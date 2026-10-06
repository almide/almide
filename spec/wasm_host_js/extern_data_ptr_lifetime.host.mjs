// The hook extern_data_ptr_lifetime.almd declares: it reads the bytes at
// the address the guest hands over. The glue gives hooks no memory handle,
// so the instance's memory is captured as it is created.
let memory = null;
const instantiate = WebAssembly.instantiate;
WebAssembly.instantiate = async (...args) => {
  const made = await instantiate.apply(WebAssembly, args);
  memory = (made.instance ?? made).exports.memory;
  return made;
};

export const js = {
  js_peek: (ptr, len) => {
    const seen = new TextDecoder().decode(new Uint8Array(memory.buffer, ptr, len));
    process.stdout.write(`${JSON.stringify(seen)}\n`);
    return len;
  },
};
