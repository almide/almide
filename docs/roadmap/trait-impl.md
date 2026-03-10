# trait / impl

Parser support is complete (`trait` / `impl` declarations are parsed and stored in AST). Checker and emitter are not yet implemented.

### Syntax (parser-complete)

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Color {
  fn show(self) -> String = match self {
    Red => "red"
    Green => "green"
    Blue => "blue"
  }
}
```

### Implementation Steps

- [ ] checker: register trait method signatures, type-check impl bodies
- [ ] checker: verify that impl provides all methods required by the trait
- [ ] Rust emitter: output `impl Trait for Type { ... }` directly
- [ ] TS emitter: output traits as interfaces, dispatch method calls
- [ ] `self` parameter handling: UFCS (both `show(color)` and `color.show()` work)

### Design Notes

- Almide has no classes. trait + impl + pattern matching is how type behavior is defined
- `self` is exclusively the receiver in trait methods, not a class instance reference
- `self` in `import self.xxx` is a separate use (distinguishable by context: import statement vs parameter list)
- `deriving` is already parser-complete (`type Color = ... deriving Show, Eq`)

---

---
