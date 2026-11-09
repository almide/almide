// ── dispatch.rs, part 7: `json.parse`'s answer where its body abstains ──
//
// include!-spliced into `dispatch.rs` at module level (the 800-line file
// discipline, #1856). The self-hosted `json_parse` (ADR-0033) carves the
// document into one Bytes family in the structural wasm leg's box layout and
// hands the top Value out through `prim.load_handle` of an interior address;
// this arena models value_core's blocks instead, so the body's SUCCESS path
// abstains here (its refusals need no family and still vote on their own).
// There the interp answers with its own parser (`json_text.rs`, the grammar
// and err texts read off the body's spec, never a runtime) and MATERIALIZES
// the document in value_core's layout, so every later self-hosted value_core
// body that takes `prim.handle` of the result reads ordinary blocks. Reached
// through `try_bridge`, so each answer is recorded for
// `interp_bridge_fallback_ledger` like every other bridge fallback.

impl<'a> Interpreter<'a> {
    /// `json.parse(text) -> Result[Value, String]`, or `None` for a call this
    /// arm does not take (the caller abstains with the body's reason).
    fn json_parse_answer(&mut self, args: &[Value]) -> Option<Flow> {
        let [Value::Str(text)] = args else {
            return None;
        };
        // The body dies (a defined abort) past this length; that path is the
        // body's to vote on, not a parse.
        if text.len() > 100_000_000 {
            return None;
        }
        Some(match crate::json_text::parse(text) {
            Ok(node) => {
                let addr = self.json_materialize(&node);
                match self.dyn_value_of(addr) {
                    Some(v) => Flow::val(Value::Result(Ok(Box::new(v)))),
                    None => Flow::Unsupported(
                        "json.parse: the materialized document does not read back".to_string(),
                    ),
                }
            }
            Err(refusal) => {
                let mut number_err = |tok: &str, float: bool| self.json_number_err(tok, float);
                match crate::json_text::message(text, &refusal, &mut number_err) {
                    Some(m) => Flow::val(Value::Result(Err(Box::new(Value::str(m))))),
                    None => Flow::Unsupported(
                        "json.parse: int.parse/float.parse gave no err text for an out-of-range number"
                            .to_string(),
                    ),
                }
            }
        })
    }

    /// The err text the interp's OWN `int.parse` (`float == false`) or
    /// `float.parse` gives `tok` — what the body passes through for an
    /// out-of-range number. An `ok` is the body's empty text; anything that is
    /// not a Result is no answer.
    fn json_number_err(&mut self, tok: &str, float: bool) -> Option<String> {
        let module = almide_base::intern::sym(if float { "float" } else { "int" });
        let parse = almide_base::intern::sym("parse");
        match self.dispatch_module_resolved(module, parse, vec![Value::str(tok)]) {
            Flow::Value(Value::Result(Err(e))) => match *e {
                Value::Str(s) => Some(s.as_str().to_string()),
                _ => None,
            },
            Flow::Value(Value::Result(Ok(_))) => Some(String::new()),
            _ => None,
        }
    }

    /// Build `node` in the block heap in value_core's layout and answer its
    /// block address: a Slots block whose `len` field is the tag (0 null,
    /// 1 bool, 2 int, 3 float bits, 4 str — slot 0 a Str block, 5 array — one
    /// slot per element Value, 6 object — (key Str, value) slot pairs), with
    /// the element / slot count in `cap` (`alloc_slots` sets it to the slot
    /// count), exactly as `dyn_node_of` reads it back.
    fn json_materialize(&mut self, node: &crate::value::DynNode) -> u32 {
        use crate::value::DynNode;
        let (tag, slots): (u32, Vec<i64>) = match node {
            DynNode::Null => (0, vec![0]),
            DynNode::Bool(b) => (1, vec![*b as i64]),
            DynNode::Int(n) => (2, vec![*n]),
            DynNode::Float(f) => (3, vec![f.to_bits() as i64]),
            DynNode::Str(s) => (4, vec![self.json_str_block(s) as i64]),
            DynNode::Arr(xs) => {
                let mut slots = Vec::with_capacity(xs.len());
                for x in xs {
                    slots.push(self.json_materialize(x) as i64);
                }
                (5, slots)
            }
            DynNode::Obj(pairs) => {
                let mut slots = Vec::with_capacity(2 * pairs.len());
                for (k, v) in pairs {
                    slots.push(self.json_str_block(k) as i64);
                    slots.push(self.json_materialize(v) as i64);
                }
                (6, slots)
            }
        };
        let base = self.heap.alloc_slots(slots.len() as u32);
        self.heap.put_len(base, tag);
        for (i, s) in slots.into_iter().enumerate() {
            self.heap.store(base + crate::heap::PAYLOAD + 8 * i as u32, 8, s);
        }
        base
    }

    /// A fresh Str block holding `s`'s bytes.
    fn json_str_block(&mut self, s: &str) -> u32 {
        let base = self.heap.alloc(s.len() as u32, crate::heap::BlockKind::Str);
        for (i, b) in s.bytes().enumerate() {
            self.heap.store(base + crate::heap::PAYLOAD + i as u32, 1, b as i64);
        }
        base
    }
}

#[cfg(test)]
mod json_materialize_tests {
    use crate::value::{DynNode, Value};
    use crate::{Flow, Interpreter};

    fn answer(interp: &mut Interpreter<'_>, text: &str) -> Result<DynNode, String> {
        match interp.json_parse_answer(&[Value::str(text)]) {
            Some(Flow::Value(Value::Result(Ok(v)))) => match *v {
                Value::Dyn { addr, node } => {
                    // The carrier's snapshot IS the arena's reading of its block.
                    let again = interp.dyn_node_of(addr as u32, 0).expect("the block reads back");
                    assert_eq!(*node, again);
                    Ok(again)
                }
                _ => panic!("json.parse answered a non-Dyn Value"),
            },
            Some(Flow::Value(Value::Result(Err(e)))) => match *e {
                Value::Str(s) => Err(s.as_str().to_string()),
                _ => panic!("json.parse answered a non-String err"),
            },
            _ => panic!("json.parse gave no Result"),
        }
    }

    #[test]
    fn a_parsed_document_round_trips_through_the_block_heap() {
        let program = almide_ir::IrProgram::default();
        let mut interp = Interpreter::new(&program);
        let doc = "{\"k\": [null, true, false, -7, 2.5, \"s\\n\\u00e9\", [], {}], \"\": {\"x\": 1e2}}";
        let want = crate::json_text::parse(doc).expect("a document");
        assert_eq!(answer(&mut interp, doc), Ok(want));
        let deep = format!(
            "{}{}",
            "[".repeat(almide_base::json_limits::JSON_MAX_DEPTH),
            "]".repeat(almide_base::json_limits::JSON_MAX_DEPTH)
        );
        assert!(answer(&mut interp, &deep).is_ok(), "the deepest admitted document reads back");
    }

    #[test]
    fn a_refusal_is_the_err_text() {
        let program = almide_ir::IrProgram::default();
        let mut interp = Interpreter::new(&program);
        assert_eq!(answer(&mut interp, "[1,]"), Err("json: trailing comma at pos 2".to_string()));
        assert_eq!(answer(&mut interp, "{\"a\":1,\"a\":2}"), Err("json: duplicate key at pos 7".to_string()));
    }
}
