# frozen_string_literal: true
#
# Shared plumbing for the lang-bench drivers (runner_multi.rb, runner_edits.rb):
# subprocess execution with a PATH extension, the `claude -p` session wrapper,
# the PASSED:/FAILED: test-suite reader and the LOC counter.
#
# Behaviour is exactly what runner_multi.rb carried inline before the extraction;
# the only difference is that the model name and the exclude list are parameters
# with the old values as defaults.

require 'json'
require 'fileutils'
require 'open3'
require 'timeout'
require 'shellwords'

BENCH_DIR     = File.expand_path('..', __dir__)
BENCH_NPM_BIN = File.join(BENCH_DIR, '.npm-prefix', 'node_modules', '.bin')

BENCH_EXCLUDE_DIR_FRAGMENTS =
  %w[/node_modules/ /target/ /build/ /_build/ /.minigit/ /deps/ /zig-out/ /.zig-cache/].freeze

# The cross-language comparison set of #1963. `probe` is the executable whose
# absence makes a row `harness-limitation` instead of a measurement.
BENCH_XLANG_LANGUAGES = {
  'almide'     => { display: 'Almide',     exts: %w[almd], version_cmd: 'almide --version', probe: 'almide',
                    copy_cheatsheet: true },
  'rust'       => { display: 'Rust',       exts: %w[rs],   version_cmd: 'rustc --version',  probe: 'rustc' },
  'go'         => { display: 'Go',         exts: %w[go],   version_cmd: 'go version',       probe: 'go' },
  'typescript' => { display: 'TypeScript', exts: %w[ts],   version_cmd: 'tsx --version',    probe: 'tsx' },
  'zig'        => { display: 'Zig',        exts: %w[zig],  version_cmd: 'zig version',      probe: 'zig' },
}.freeze

def bench_extra_path
  "#{File.join(Dir.home, '.moon', 'bin')}:#{BENCH_NPM_BIN}:/opt/homebrew/bin"
end

def run_cmd(cmd, dir: nil, timeout: 600, extra_path: bench_extra_path)
  opts = {}
  opts[:chdir] = dir if dir
  stdin, stdout, stderr, wait_thr = Open3.popen3("export PATH=#{extra_path}:$PATH && #{cmd}", **opts)
  stdin.close
  stdout.set_encoding('UTF-8')
  stderr.set_encoding('UTF-8')
  out = err = +''
  begin
    Timeout.timeout(timeout) do
      out = stdout.read
      err = stderr.read
    end
  rescue Timeout::Error
    (Process.kill('TERM', wait_thr.pid) rescue nil)
    out = (stdout.read rescue '')
    err = "Timeout after #{timeout}s"
  end
  stdout.close
  stderr.close
  status = wait_thr.value
  { stdout: out, stderr: err, exit_code: status.exitstatus, success: status.success? }
end

def parse_claude_json(raw)
  raw = raw.dup.force_encoding('UTF-8')
  events = JSON.parse(raw.strip)
  events = [events] unless events.is_a?(Array)
  result = events.reverse.find { |e| e.is_a?(Hash) && e['type'] == 'result' }
  return nil unless result

  usage = result['usage'] || {}
  {
    'input_tokens' => usage['input_tokens'] || 0,
    'output_tokens' => usage['output_tokens'] || 0,
    'cache_creation_tokens' => usage['cache_creation_input_tokens'] || 0,
    'cache_read_tokens' => usage['cache_read_input_tokens'] || 0,
    'cost_usd' => result['total_cost_usd'] || 0.0,
    'num_turns' => result['num_turns'] || 0,
    'duration_ms' => result['duration_ms'] || 0,
  }
rescue JSON::ParserError => e
  warn "warn: failed to parse Claude JSON: #{e.message}"
  nil
end

def run_claude(prompt, dir:, model:, log_path: nil, timeout: 1800, extra_path: bench_extra_path)
  cmd = "unset CLAUDECODE && claude -p #{Shellwords.escape(prompt)} " \
        "--dangerously-skip-permissions --output-format json --model #{model}"
  puts "  Running Claude (#{model})..."
  t0 = Time.now
  result = run_cmd(cmd, dir: dir, timeout: timeout, extra_path: extra_path)
  elapsed = (Time.now - t0).round(1)
  if log_path
    FileUtils.mkdir_p(File.dirname(log_path))
    File.write(log_path, result[:stdout])
  end
  {
    success: result[:success],
    elapsed: elapsed,
    claude_data: parse_claude_json(result[:stdout]),
  }
end

# Runs a shell suite that reports `PASSED: n` / `FAILED: m` (the upstream
# test-v1.sh idiom). `script` may be a path relative to `dir` or absolute.
def run_tests(script, dir:, timeout: 300, extra_path: bench_extra_path)
  result = run_cmd("bash #{Shellwords.escape(script)}", dir: dir, timeout: timeout, extra_path: extra_path)
  output = result[:stdout] + result[:stderr]
  passed = output[/PASSED:\s*(\d+)/, 1]&.to_i || 0
  failed = output[/FAILED:\s*(\d+)/, 1]&.to_i || 0
  { success: result[:success], passed: passed, failed: failed, total: passed + failed, output: output }
end

# Non-blank source lines under `dir` for the given extensions. `script_name`
# names an extensionless executable that may BE the source (scripting
# languages); pass nil when the task has no such file.
def count_loc(dir, exts, exclude: BENCH_EXCLUDE_DIR_FRAGMENTS, script_name: 'minigit')
  files = exts.flat_map { |e| Dir.glob(File.join(dir, '**', "*.#{e}")) }
  files.reject! { |f| exclude.any? { |frag| f.include?(frag) } }

  if script_name
    script = File.join(dir, script_name)
    if File.exist?(script) && !files.include?(script)
      begin
        content = File.read(script, encoding: 'UTF-8')
        files << script if content.valid_encoding?
      rescue StandardError
        # skip binary
      end
    end
  end

  files.sum do |f|
    File.readlines(f).count { |l| !l.strip.empty? }
  rescue StandardError
    0
  end
end
