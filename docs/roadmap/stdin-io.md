# stdin / Interactive I/O ✅ Implemented

## API

```almide
import io

effect fn main() -> Result[Unit, String] = {
  io.print("Enter weight (kg): ")
  let weight_str = io.read_line()
  let weight = float.parse(weight_str)
  // ...
}
```

## stdlib functions

```
io.read_line() -> String          // read one line from stdin (blocking), effect fn
io.print(s: String)               // print without newline (stdout), effect fn
io.read_all() -> String           // read all of stdin, effect fn
```

- All are effect fns (require `effect fn` context)
- `io.print` complements the existing `println` (which always adds newline)
- Rust emitter: uses `std::io::BufRead` / `std::io::Write`
- TS emitter: Deno → `prompt()` / `Deno.stdout.writeSync`, Node → `fs.readSync(0, ...)` / `process.stdout.write`
