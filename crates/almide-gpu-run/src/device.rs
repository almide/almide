//! The wgpu runner: one adapter, one queue, one kernel dispatch per call.
//!
//! An `Int` crosses the boundary as its little-endian 8 bytes, which is the
//! `vec2<u32>` (low word first) layout the kernel reads — no conversion.

use crate::kernel::{ENTRY_POINT, WORKGROUP_SIZE};
use std::sync::mpsc;

/// The most elements one 1-D dispatch covers: WebGPU guarantees 65,535
/// workgroups per dimension.
pub const MAX_ELEMENTS: usize = 65_535 * WORKGROUP_SIZE as usize;

const INT_BYTES: u64 = 8;

/// Why a dispatch did not produce a result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuError {
    /// No adapter on this machine (no GPU and no software rasterizer).
    NoAdapter(String),
    /// The adapter refused a device.
    NoDevice(String),
    /// The kernel failed WGSL validation — an emitter bug, never user error.
    Validation(String),
    /// The input exceeds one dispatch.
    TooLarge(usize),
    /// Mapping the result back failed.
    Readback(String),
}

impl std::fmt::Display for GpuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GpuError::NoAdapter(m) => write!(f, "no GPU adapter: {m}"),
            GpuError::NoDevice(m) => write!(f, "the adapter refused a device: {m}"),
            GpuError::Validation(m) => write!(f, "the kernel failed WGSL validation: {m}"),
            GpuError::TooLarge(n) => write!(f, "{n} elements exceed one dispatch ({MAX_ELEMENTS})"),
            GpuError::Readback(m) => write!(f, "reading the result back failed: {m}"),
        }
    }
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// `name (backend, device type)` of the adapter, for reports.
    pub adapter: String,
}

impl Gpu {
    /// Open the default adapter. A software adapter (e.g. lavapipe) counts:
    /// it runs the same kernel, so it is enough for the parity check.
    pub fn new() -> Result<Gpu, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| GpuError::NoDevice(e.to_string()))?;
        Ok(Gpu { device, queue, adapter: format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type) })
    }

    /// Run `wgsl` (from [`crate::kernel::int_map_kernel`]) over `xs`.
    pub fn run_int_map(&self, wgsl: &str, xs: &[i64]) -> Result<Vec<i64>, GpuError> {
        if xs.is_empty() {
            return Ok(Vec::new());
        }
        if xs.len() > MAX_ELEMENTS {
            return Err(GpuError::TooLarge(xs.len()));
        }
        let size = xs.len() as u64 * INT_BYTES;
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("almide int map"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("almide int map"),
            layout: None,
            module: &module,
            entry_point: Some(ENTRY_POINT),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(e) = pollster::block_on(scope.pop()) {
            return Err(GpuError::Validation(e.to_string()));
        }
        let buffer = |label, usage| {
            self.device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: false })
        };
        let input = buffer("xs", wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        let output = buffer("ys", wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
        let readback = buffer("readback", wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
        let bytes: Vec<u8> = xs.iter().flat_map(|x| x.to_le_bytes()).collect();
        self.queue.write_buffer(&input, 0, &bytes);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: input.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: output.as_entire_binding() },
            ],
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups((xs.len() as u32).div_ceil(WORKGROUP_SIZE), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
        self.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: None })
            .map_err(|e| GpuError::Readback(e.to_string()))?;
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(GpuError::Readback(e.to_string())),
            Err(e) => return Err(GpuError::Readback(e.to_string())),
        }
        let ys = {
            let view = readback.get_mapped_range(..).map_err(|e| GpuError::Readback(e.to_string()))?;
            view.chunks_exact(INT_BYTES as usize)
                .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
                .collect()
        };
        readback.unmap();
        Ok(ys)
    }
}
