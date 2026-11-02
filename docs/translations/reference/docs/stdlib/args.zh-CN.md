# args

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](args.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](args.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](args.zh-CN.md)



> 原文：[args.md @ 35297a6](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/stdlib/args.md)。这是供读者参考的非规范性固定快照。API 索引取自固定版本原文，本译文不参与自动生成或自动同步。签名、类型、`@since` 及代码示例（包括注释）保持原文，API 说明译文列于文末。本次翻译未独立执行代码示例。

基于 `env.args()` 的命令行参数解析。`import args`。

`args` 是轻量、分配较少的读取器，没有解析器对象，也没有模式。每次调用都会重新读取进程参数，调用顺序任意。

所有读取器都是 `effect fn`（dialect epoch 13）：参数是输入，而普通 `fn` 从不读取输入。请从 `effect fn` 调用，并像其他 effect fn 调用一样用 `!` 传播；或者在 `effect fn main` 中读取一次，再将值传给普通 fn。

程序参数放在 `--` 后：

```bash
almide run app.almd -- --verbose --output=out.txt input.csv
```

### `args.option(name: String) -> Option[String]`

读取 `--` 前的 `--name` 的值，接受 `--name=value` 和 `--name value` 两种写法。标志不存在或后面没有值时返回 `none`。

```almd check
import args
import fs

effect fn main() -> Unit = {
  let body = "report body"
  match args.option("output")! {
    some(path) => fs.write_text(path, body)!,
    none => println(body),
  }
}
```

### `args.positional(options: List[String]) -> List[String]`

返回 `env.args()`（已经排除 argv[0]）中不是标志的参数，随后附上 `--` 后的所有参数。`options` 列出程序通过 `args.option` 读取的每个选项名称：`--name` 后的参数是选项值，不是位置参数，因此对 `--out report.txt input.txt` 传入 `["out"]` 得到 `["input.txt"]`。程序不读取选项时传入 `[]`。

```almd check
import args

fn process(file: String) -> Unit = println("processing ${file}")

effect fn main() -> Unit = {
  let out = args.option("out")! ?? "report.txt"
  for file in args.positional(["out"])! { process(file) }
  println("writing ${out}")
}
```



## 固定快照 API 索引（3个函数）

```
// True on --name before `--`; a one-letter name also matches -name. No short form is derived.
// @since unreleased
effect args.has_flag(name: String) -> Bool

// Value of --name=v or --name v before `--`; none if absent.
// @since 0.2.0 or earlier
effect args.option(name: String) -> Option[String]

// Args before `--` that are not flags or values of the named options, then all after `--`.
// @since 0.2.0 or earlier
effect args.positional(options: List[String]) -> List[String]
```




## API 说明译文（按上述签名顺序）

- `effect args.has_flag(name: String) -> Bool` — `--` 前出现 --name 则为 true；单字母名称也匹配 -name。不推导短形式。
- `effect args.option(name: String) -> Option[String]` — `--` 前 --name=v 或 --name v 的值；不存在则为 none。
- `effect args.positional(options: List[String]) -> List[String]` — `--` 前既非标志也非指定选项值的参数，随后为 `--` 后的所有参数。
