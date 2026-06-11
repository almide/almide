//! WASM runtime: regex matching engine.
//!
//! A node-graph backtracking interpreter that mirrors the NATIVE oracle
//! (`runtime/rs/src/regex.rs`) structure-for-structure so the two stay
//! byte-identical and future native changes port mechanically.
//!
//! ## Why a node graph (not a byte-walker)
//!
//! The previous wasm engine re-scanned the *pattern string* on every recursion
//! step. That design had no concept of alternation (`a|b`), no lexical capture
//! indices, and matched one *byte* of a multibyte scalar at a time. To match
//! native exactly we PARSE THE PATTERN ONCE per public call (mirroring native's
//! `rx_compile`, which is also called fresh per call — native does not cache)
//! into a node graph in linear memory, then run a recursive backtracking
//! matcher over it. The matcher reads the haystack by UTF-8 SCALAR
//! (`utf8_scalar`/`utf8_width`) and keeps haystack positions as BYTE offsets so
//! `string.slice` (byte-indexed) stays correct, advancing by scalar width.
//!
//! ## Memory model (linked node graph — strictly O(pattern size))
//!
//! `__rx_compile(pat) -> alts_ptr` allocates one arena (a bump region) and emits
//! records. Native `Vec<Box<…>>` nesting maps to singly-linked lists so no
//! contiguous-slot reservation (and no interleave bug) is needed. Every field is
//! a 4-byte i32 word; all links are absolute byte addresses (0 = null).
//!
//! * `Alts`  : `[head_seq_ptr]`              — first Seq (or 0 if none)
//! * `Seq`   : `[head_piece_ptr, next_seq]`  — first Piece + next alternative
//! * `Piece` : `[kind, min, max, x, y, z, next_piece]`
//!     - LIT          : x = scalar
//!     - DOT          : —
//!     - CLASS        : x = ranges_ptr, y = nranges, z = negated(0/1)
//!     - ANCHOR_START : —
//!     - ANCHOR_END   : —
//!     - GROUP        : x = inner alts_ptr, z = capture_index (1-based; 0 = none)
//! * `Range` : `[lo, hi]` scalar pair (CLASS ranges, packed contiguously)
//!
//! `max` uses RX_MAX_UNBOUNDED (-1) for the native `None`.
//!
//! ## Capture backtracking
//!
//! Captures live in a `caps` buffer of `ncap` slots, each `[start, end]` BYTE
//! offsets (-1 = unset). Native clones `caps` on every alternation and every
//! repetition branch and restores on failure; we mirror that with a bump "save
//! stack" (`rx_save_sp_global`) carved from the same arena: a branch pushes a
//! `ncap*2`-word copy and restores by copying back on failure.

use super::{CompiledFunc, WasmEmitter};
use wasm_encoder::ValType;
use super::TrackedFunction as Function;
use super::rt_string::string_data_off;

// ─── Node-kind tags (mirror native `RxNode` variants) ───
const RX_KIND_LIT: i32 = 0;
const RX_KIND_DOT: i32 = 1;
const RX_KIND_CLASS: i32 = 2;
const RX_KIND_ANCHOR_START: i32 = 3;
const RX_KIND_ANCHOR_END: i32 = 4;
const RX_KIND_GROUP: i32 = 5;

const RX_WORD: i32 = 4;

// ─── Record sizes (in 4-byte words) ───
const RX_ALTS_WORDS: i32 = 1; // [head_seq]
const RX_SEQ_WORDS: i32 = 2; // [head_piece, next_seq]
const RX_PIECE_WORDS: i32 = 7; // [kind, min, max, x, y, z, next_piece]
const RX_RANGE_WORDS: i32 = 2; // [lo, hi]
const RX_CAP_WORDS: i32 = 2; // [start, end]

// ─── Alts field offsets ───
const RX_ALTS_HEAD_OFF: i32 = 0;
// ─── Seq field offsets ───
const RX_SEQ_HEAD_OFF: i32 = 0;
const RX_SEQ_NEXT_OFF: i32 = 1 * RX_WORD;
// ─── Piece field offsets ───
const RX_PIECE_KIND_OFF: i32 = 0;
const RX_PIECE_MIN_OFF: i32 = 1 * RX_WORD;
const RX_PIECE_MAX_OFF: i32 = 2 * RX_WORD;
const RX_PIECE_X_OFF: i32 = 3 * RX_WORD;
const RX_PIECE_Y_OFF: i32 = 4 * RX_WORD;
const RX_PIECE_Z_OFF: i32 = 5 * RX_WORD;
const RX_PIECE_NEXT_OFF: i32 = 6 * RX_WORD;
// ─── Range field offsets ───
const RX_RANGE_LO_OFF: i32 = 0;
const RX_RANGE_HI_OFF: i32 = 1 * RX_WORD;
// ─── Cap slot field offsets ───
const RX_CAP_START_OFF: i32 = 0;
const RX_CAP_END_OFF: i32 = 1 * RX_WORD;
const RX_CAP_BYTES: i32 = RX_CAP_WORDS * RX_WORD;

/// `max == None` sentinel (native `Option<usize>::None`).
const RX_MAX_UNBOUNDED: i32 = -1;
/// `caps` slot sentinel for "unset" (native `Option<(usize,usize)>::None`).
const RX_CAP_UNSET: i32 = -1;
/// `__rx_match_*` / `__rx_find_at` failure sentinel (native `None`).
const RX_NO_MATCH: i32 = -1;
/// Null link.
const RX_NULL: i32 = 0;

// ─── ASCII byte constants used by the parser (named, not magic) ───
const ASCII_PIPE: i32 = b'|' as i32;
const ASCII_LPAREN: i32 = b'(' as i32;
const ASCII_RPAREN: i32 = b')' as i32;
const ASCII_STAR: i32 = b'*' as i32;
const ASCII_PLUS: i32 = b'+' as i32;
const ASCII_QUESTION: i32 = b'?' as i32;
const ASCII_DOT: i32 = b'.' as i32;
const ASCII_CARET: i32 = b'^' as i32;
const ASCII_DOLLAR: i32 = b'$' as i32;
const ASCII_BACKSLASH: i32 = b'\\' as i32;
const ASCII_LBRACKET: i32 = b'[' as i32;
const ASCII_RBRACKET: i32 = b']' as i32;
const ASCII_DASH: i32 = b'-' as i32;
const ASCII_LOWER_D: i32 = b'd' as i32;
const ASCII_UPPER_D: i32 = b'D' as i32;
const ASCII_LOWER_W: i32 = b'w' as i32;
const ASCII_UPPER_W: i32 = b'W' as i32;
const ASCII_LOWER_S: i32 = b's' as i32;
const ASCII_UPPER_S: i32 = b'S' as i32;
const ASCII_LOWER_N: i32 = b'n' as i32;
const ASCII_LOWER_T: i32 = b't' as i32;
const ASCII_LOWER_R: i32 = b'r' as i32;
const ASCII_NEWLINE: i32 = b'\n' as i32;
const ASCII_TAB: i32 = b'\t' as i32;
const ASCII_CR: i32 = b'\r' as i32;
const ASCII_SPACE: i32 = b' ' as i32;

// Class range bounds for `\d` / `\w` / `\s` (ASCII-only, matching native).
const RX_DIGIT_LO: i32 = b'0' as i32;
const RX_DIGIT_HI: i32 = b'9' as i32;
const RX_LOWER_LO: i32 = b'a' as i32;
const RX_LOWER_HI: i32 = b'z' as i32;
const RX_UPPER_LO: i32 = b'A' as i32;
const RX_UPPER_HI: i32 = b'Z' as i32;
const RX_UNDERSCORE: i32 = b'_' as i32;

/// Arena sizing: words reserved per pattern byte plus a fixed slack. Each
/// pattern byte yields at most one Piece (7 words) plus a Seq/Alts header and a
/// range pair; `\w` expands to 4 ranges (8 words). The per-call save stack needs
/// depth × ncap × 2 words; `× 24` + slack covers the worst realistic pattern as
/// one up-front allocation. Native allocates per call too (no cache), so a fresh
/// arena per call mirrors native memory behavior.
const RX_ARENA_WORDS_PER_BYTE: i32 = 24;
const RX_ARENA_SLACK_WORDS: i32 = 512;
/// Fraction of the arena handed to the node graph; the rest is the save stack.
/// Nodes are bounded by O(pattern); the save stack by O(depth × ncap).
const RX_ARENA_NODE_SHIFT: i32 = 1; // node region = arena/2, save region = arena/2

/// Regex-related runtime function indices.
#[derive(Default, Clone)]
pub struct RegexRuntime {
    // ── Parser ──
    pub compile: u32,
    pub parse_alts: u32,
    pub parse_piece: u32,
    pub parse_atom: u32,
    pub parse_escape: u32,
    pub parse_class: u32,
    // ── Matcher ──
    pub node_matches: u32,
    pub match_alts: u32,
    pub match_seq: u32,
    pub match_rep: u32,
    pub match_one: u32,
    pub find_at: u32,
    // ── Globals ──
    /// Bump cursor (byte addr) for the per-call node arena.
    pub arena_sp_global: u32,
    /// Bump cursor (byte addr) for the per-call capture save stack.
    pub save_sp_global: u32,
    /// Parser cursor into the pattern bytes (mutable `*pos`).
    pub parse_pos_global: u32,
    /// Group count assigned during parse (`ncap`).
    pub ncap_global: u32,
    /// Start byte offset of the last successful search (`__rx_find_at`).
    pub match_start_global: u32,
}

pub(super) fn register(emitter: &mut WasmEmitter) {
    let ty1 = emitter.register_type(vec![ValType::I32], vec![ValType::I32]);
    let ty2 = emitter.register_type(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    // `parse_escape` / `parse_class` write the node into the Piece in place and
    // return nothing — a distinct `(i32,i32) -> ()` signature.
    let ty2_void = emitter.register_type(vec![ValType::I32, ValType::I32], vec![]);
    let ty5 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );
    let ty6 = emitter.register_type(
        vec![ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32, ValType::I32],
        vec![ValType::I32],
    );

    // ── Parser (registration order = the index space callers reference) ──
    emitter.rt.regex.compile = emitter.register_func("__rx_compile", ty1);
    emitter.rt.regex.parse_alts = emitter.register_func("__rx_parse_alts", ty2);
    emitter.rt.regex.parse_piece = emitter.register_func("__rx_parse_piece", ty1);
    emitter.rt.regex.parse_atom = emitter.register_func("__rx_parse_atom", ty1);
    emitter.rt.regex.parse_escape = emitter.register_func("__rx_parse_escape", ty2_void);
    emitter.rt.regex.parse_class = emitter.register_func("__rx_parse_class", ty2_void);
    // ── Matcher ──
    emitter.rt.regex.node_matches = emitter.register_func("__rx_node_matches", ty2);
    emitter.rt.regex.match_alts = emitter.register_func("__rx_match_alts", ty5);
    // match_seq(piece, text, p, caps, ncap) = 5 params; match_rep adds `count`.
    emitter.rt.regex.match_seq = emitter.register_func("__rx_match_seq", ty5);
    emitter.rt.regex.match_rep = emitter.register_func("__rx_match_rep", ty6);
    emitter.rt.regex.match_one = emitter.register_func("__rx_match_one", ty5);
    emitter.rt.regex.find_at = emitter.register_func("__rx_find_at", ty5);

    // ── Globals ──
    emitter.rt.regex.arena_sp_global = next_i32_global(emitter);
    emitter.rt.regex.save_sp_global = next_i32_global(emitter);
    emitter.rt.regex.parse_pos_global = next_i32_global(emitter);
    emitter.rt.regex.ncap_global = next_i32_global(emitter);
    emitter.rt.regex.match_start_global = next_i32_global(emitter);
}

fn next_i32_global(emitter: &mut WasmEmitter) -> u32 {
    let g = emitter.next_global;
    emitter.next_global += 1;
    emitter.top_let_init.push((g, ValType::I32, 0));
    g
}

pub(super) fn compile(emitter: &mut WasmEmitter) {
    // Order matches register() so each function's index is defined before any
    // body emits a `call` to it. The parser is mutually recursive
    // (alts→piece→atom→alts) and the matcher is mutually recursive
    // (alts→seq→rep→one→alts); WASM resolves call indices via the function
    // section, so any compile order works once register() has run.
    compile_compile(emitter);
    compile_parse_alts(emitter);
    compile_parse_piece(emitter);
    compile_parse_atom(emitter);
    compile_parse_escape(emitter);
    compile_parse_class(emitter);

    compile_node_matches(emitter);
    compile_match_alts(emitter);
    compile_match_seq(emitter);
    compile_match_rep(emitter);
    compile_match_one(emitter);
    compile_find_at(emitter);
}

// ════════════════════════════════════════════════════════════════════════
//  Parser
// ════════════════════════════════════════════════════════════════════════

/// Emit: bump the node arena by `n` (const) words, leaving the base byte-addr
/// on the WASM stack.
fn emit_node_alloc(f: &mut Function, arena_sp: u32, n: i32) {
    wasm!(f, {
        global_get(arena_sp);                              // base (result)
        global_get(arena_sp); i32_const(n * RX_WORD); i32_add; global_set(arena_sp);
    });
}

// ─── __rx_compile(pat) -> alts_ptr ───
// Mirrors native rx_compile: reset cursors, ncap=0, parse top-level alts.
fn compile_compile(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.compile];
    // params: 0=pat | locals: 1=pat_len, 2=arena_base, 3=arena_words, 4=node_words
    let mut f = Function::new([(4, ValType::I32)]);
    let alloc = emitter.rt.alloc;
    let parse_alts = emitter.rt.regex.parse_alts;
    let arena_sp = emitter.rt.regex.arena_sp_global;
    let save_sp = emitter.rt.regex.save_sp_global;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let ncap = emitter.rt.regex.ncap_global;

    wasm!(f, {
        local_get(0); i32_load(0); local_set(1);
        // arena_words = pat_byte_len * PER_BYTE + SLACK
        local_get(1); i32_const(RX_ARENA_WORDS_PER_BYTE); i32_mul;
        i32_const(RX_ARENA_SLACK_WORDS); i32_add; local_set(3);
        // arena_base = alloc(arena_words * 4)
        local_get(3); i32_const(RX_WORD); i32_mul; call(alloc); local_set(2);
        // node region grows up from arena_base; save stack grows up from the
        // node/save split point so the two bump cursors never collide.
        local_get(2); global_set(arena_sp);
        local_get(3); i32_const(RX_ARENA_NODE_SHIFT); i32_shr_u; local_set(4); // node_words = arena_words/2
        local_get(2); local_get(4); i32_const(RX_WORD); i32_mul; i32_add; global_set(save_sp);
        // reset parser state
        i32_const(0); global_set(parse_pos);
        i32_const(0); global_set(ncap);
        // alts = parse_alts(pat, in_group=0)
        local_get(0); i32_const(0); call(parse_alts);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.compile, type_idx, f));
}

// ─── __rx_parse_alts(pat, in_group) -> alts_ptr ───
// Mirrors native rx_parse_alts: split on '|' into Seqs, stop at ')' when
// in_group. Builds a linked list of Seqs; each Seq a linked list of Pieces.
fn compile_parse_alts(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.parse_alts];
    // params: 0=pat, 1=in_group
    // locals: 2=pat_len, 3=alts_ptr, 4=cur_seq, 5=last_seq, 6=cur_piece_tail,
    //         7=ch, 8=piece, 9=new_seq
    let mut f = Function::new([(8, ValType::I32)]);
    let parse_piece = emitter.rt.regex.parse_piece;
    let arena_sp = emitter.rt.regex.arena_sp_global;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let data_off = string_data_off();

    wasm!(f, { local_get(0); i32_load(0); local_set(2); });
    // Alts header
    emit_node_alloc(&mut f, arena_sp, RX_ALTS_WORDS);
    wasm!(f, { local_set(3); });
    // First Seq (native starts with one empty alt). cur_seq head=null, next=null
    emit_node_alloc(&mut f, arena_sp, RX_SEQ_WORDS);
    wasm!(f, {
        local_set(4); // cur_seq
        local_get(4); i32_const(RX_SEQ_HEAD_OFF); i32_add; i32_const(RX_NULL); i32_store(0);
        local_get(4); i32_const(RX_SEQ_NEXT_OFF); i32_add; i32_const(RX_NULL); i32_store(0);
        local_get(3); i32_const(RX_ALTS_HEAD_OFF); i32_add; local_get(4); i32_store(0);
        local_get(4); local_set(5); // last_seq = cur_seq
        i32_const(RX_NULL); local_set(6); // cur_piece_tail = null
    });
    // Main loop
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { global_get(parse_pos); local_get(2); i32_ge_u; br_if(1); });
    wasm!(f, {
        local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0); local_set(7);
    });
    // if in_group && ch==')' break
    wasm!(f, {
        local_get(1); local_get(7); i32_const(ASCII_RPAREN); i32_eq; i32_and;
        if_empty; br(2); end;
    });
    // if ch=='|' : pos++, start a new (possibly empty) Seq; else parse a piece.
    // The two arms share a single `br(0)` (continue loop) emitted AFTER the if,
    // at loop level — a `br(0)` inside the `if` would target the `if`'s own end
    // (not the loop) and fall through into the piece path, spuriously appending
    // an atom to a freshly-started empty trailing arm (`a|`). Native mirrors this
    // as `if '|' { … continue } else { push piece }` in rx_parse_alts.
    wasm!(f, {
        local_get(7); i32_const(ASCII_PIPE); i32_eq;
        if_empty;
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
    });
    emit_node_alloc(&mut f, arena_sp, RX_SEQ_WORDS);
    wasm!(f, {
            local_set(9); // new_seq
            local_get(9); i32_const(RX_SEQ_HEAD_OFF); i32_add; i32_const(RX_NULL); i32_store(0);
            local_get(9); i32_const(RX_SEQ_NEXT_OFF); i32_add; i32_const(RX_NULL); i32_store(0);
            // last_seq.next = new_seq
            local_get(5); i32_const(RX_SEQ_NEXT_OFF); i32_add; local_get(9); i32_store(0);
            local_get(9); local_set(5); // last_seq = new_seq
            local_get(9); local_set(4); // cur_seq = new_seq
            i32_const(RX_NULL); local_set(6); // reset piece tail
        else_;
            // piece = parse_piece(pat); append to cur_seq's piece list
            local_get(0); call(parse_piece); local_set(8);
            local_get(8); i32_const(RX_PIECE_NEXT_OFF); i32_add; i32_const(RX_NULL); i32_store(0);
            // if cur_piece_tail==null: cur_seq.head = piece; else tail.next = piece
            local_get(6); i32_eqz;
            if_empty;
                local_get(4); i32_const(RX_SEQ_HEAD_OFF); i32_add; local_get(8); i32_store(0);
            else_;
                local_get(6); i32_const(RX_PIECE_NEXT_OFF); i32_add; local_get(8); i32_store(0);
            end;
            local_get(8); local_set(6); // tail = piece
        end;
        br(0); // continue loop (loop level, outside the '|' if)
    });
    wasm!(f, { end; end; }); // end loop, block
    wasm!(f, { local_get(3); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.parse_alts, type_idx, f));
}

// ─── __rx_parse_piece(pat) -> piece_ptr ───
// atom then quantifier. parse_atom allocates the Piece + fills its node; here
// we read the quantifier and set min/max (default 1,1).
fn compile_parse_piece(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.parse_piece];
    // params: 0=pat | locals: 1=piece, 2=pat_len, 3=ch
    let mut f = Function::new([(3, ValType::I32)]);
    let parse_atom = emitter.rt.regex.parse_atom;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let data_off = string_data_off();

    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        local_get(0); call(parse_atom); local_set(1);
        // default (min=1, max=1)
        local_get(1); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_const(1); i32_store(0);
        local_get(1); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_const(1); i32_store(0);
        // quantifier?
        global_get(parse_pos); local_get(2); i32_lt_u;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0); local_set(3);
            // '*' => (0, None)
            local_get(3); i32_const(ASCII_STAR); i32_eq;
            if_empty;
                local_get(1); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_const(0); i32_store(0);
                local_get(1); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_const(RX_MAX_UNBOUNDED); i32_store(0);
                global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            else_;
                local_get(3); i32_const(ASCII_PLUS); i32_eq;
                if_empty;
                    local_get(1); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_const(1); i32_store(0);
                    local_get(1); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_const(RX_MAX_UNBOUNDED); i32_store(0);
                    global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
                else_;
                    local_get(3); i32_const(ASCII_QUESTION); i32_eq;
                    if_empty;
                        local_get(1); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_const(0); i32_store(0);
                        local_get(1); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_const(1); i32_store(0);
                        global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
                    end;
                end;
            end;
        end;
        local_get(1);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.parse_piece, type_idx, f));
}

// ─── __rx_parse_atom(pat) -> piece_ptr ───
// Allocates a Piece, fills its node (kind + payload), consumes the atom chars.
fn compile_parse_atom(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.parse_atom];
    // params: 0=pat | locals: 1=piece, 2=scalar, 3=width, 4=ci, 5=inner_alts,
    //         6=lead, 7=pat_len
    let mut f = Function::new([(7, ValType::I32)]);
    let arena_sp = emitter.rt.regex.arena_sp_global;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let parse_escape = emitter.rt.regex.parse_escape;
    let parse_class = emitter.rt.regex.parse_class;
    let parse_alts = emitter.rt.regex.parse_alts;
    let ncap = emitter.rt.regex.ncap_global;
    let utf8_scalar = emitter.rt.string.utf8_scalar;
    let utf8_width = emitter.rt.string.utf8_width;
    let data_off = string_data_off();

    emit_node_alloc(&mut f, arena_sp, RX_PIECE_WORDS);
    wasm!(f, { local_set(1); });
    wasm!(f, {
        local_get(0); i32_load(0); local_set(7);
        local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0); local_set(6);
    });
    // '.' => Dot
    wasm!(f, {
        local_get(6); i32_const(ASCII_DOT); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_DOT); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            local_get(1); return_;
        end;
    });
    // '^' => AnchorStart
    wasm!(f, {
        local_get(6); i32_const(ASCII_CARET); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_ANCHOR_START); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            local_get(1); return_;
        end;
    });
    // '$' => AnchorEnd
    wasm!(f, {
        local_get(6); i32_const(ASCII_DOLLAR); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_ANCHOR_END); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            local_get(1); return_;
        end;
    });
    // '\\' => escape
    wasm!(f, {
        local_get(6); i32_const(ASCII_BACKSLASH); i32_eq;
        if_empty;
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            local_get(0); local_get(1); call(parse_escape);
            local_get(1); return_;
        end;
    });
    // '[' => class
    wasm!(f, {
        local_get(6); i32_const(ASCII_LBRACKET); i32_eq;
        if_empty;
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
            local_get(0); local_get(1); call(parse_class);
            local_get(1); return_;
        end;
    });
    // '(' => group: ncap++, ci, recurse alts(in_group=1), consume ')'
    wasm!(f, {
        local_get(6); i32_const(ASCII_LPAREN); i32_eq;
        if_empty;
            global_get(ncap); i32_const(1); i32_add; local_tee(4); global_set(ncap); // ci = ++ncap
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); // consume '('
            local_get(0); i32_const(1); call(parse_alts); local_set(5);
            // consume ')' if present
            global_get(parse_pos); local_get(7); i32_lt_u;
            if_empty;
                local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0);
                i32_const(ASCII_RPAREN); i32_eq;
                if_empty; global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); end;
            end;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_GROUP); i32_store(0);
            local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; local_get(5); i32_store(0);
            local_get(1); i32_const(RX_PIECE_Z_OFF); i32_add; local_get(4); i32_store(0);
            local_get(1); return_;
        end;
    });
    // else: literal scalar (decode by UTF-8, advance by width)
    wasm!(f, {
        local_get(0); global_get(parse_pos); call(utf8_scalar); i32_wrap_i64; local_set(2);
        local_get(0); global_get(parse_pos); call(utf8_width); local_set(3);
        local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
        local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; local_get(2); i32_store(0);
        global_get(parse_pos); local_get(3); i32_add; global_set(parse_pos);
        local_get(1);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.parse_atom, type_idx, f));
}

// ─── __rx_parse_escape(pat, piece_ptr) -> () ───
// parse_pos points AT the escape char (after the backslash). Writes the node
// into piece_ptr, advances parse_pos. Mirrors native rx_parse_escape.
fn compile_parse_escape(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.parse_escape];
    // params: 0=pat, 1=piece | locals: 2=pat_len, 3=c, 4=ranges, 5=width
    let mut f = Function::new([(4, ValType::I32)]);
    let arena_sp = emitter.rt.regex.arena_sp_global;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let utf8_scalar = emitter.rt.string.utf8_scalar;
    let utf8_width = emitter.rt.string.utf8_width;
    let data_off = string_data_off();

    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        // if pos >= pat_len → Lit('\\')
        global_get(parse_pos); local_get(2); i32_ge_u;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
            local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; i32_const(ASCII_BACKSLASH); i32_store(0);
            return_;
        end;
        local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0); local_set(3);
    });

    // \d / \D
    wasm!(f, { local_get(3); i32_const(ASCII_LOWER_D); i32_eq; if_empty; });
    emit_class_digit(&mut f, arena_sp, 1, 4, 0);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    wasm!(f, { local_get(3); i32_const(ASCII_UPPER_D); i32_eq; if_empty; });
    emit_class_digit(&mut f, arena_sp, 1, 4, 1);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    // \w / \W
    wasm!(f, { local_get(3); i32_const(ASCII_LOWER_W); i32_eq; if_empty; });
    emit_class_word(&mut f, arena_sp, 1, 4, 0);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    wasm!(f, { local_get(3); i32_const(ASCII_UPPER_W); i32_eq; if_empty; });
    emit_class_word(&mut f, arena_sp, 1, 4, 1);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    // \s / \S
    wasm!(f, { local_get(3); i32_const(ASCII_LOWER_S); i32_eq; if_empty; });
    emit_class_space(&mut f, arena_sp, 1, 4, 0);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    wasm!(f, { local_get(3); i32_const(ASCII_UPPER_S); i32_eq; if_empty; });
    emit_class_space(&mut f, arena_sp, 1, 4, 1);
    wasm!(f, { global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_; end; });
    // \n \t \r → control Lit
    wasm!(f, {
        local_get(3); i32_const(ASCII_LOWER_N); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
            local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; i32_const(ASCII_NEWLINE); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_;
        end;
        local_get(3); i32_const(ASCII_LOWER_T); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
            local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; i32_const(ASCII_TAB); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_;
        end;
        local_get(3); i32_const(ASCII_LOWER_R); i32_eq;
        if_empty;
            local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
            local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; i32_const(ASCII_CR); i32_store(0);
            global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); return_;
        end;
    });
    // default → Lit(escaped scalar), advance by width
    wasm!(f, {
        local_get(0); global_get(parse_pos); call(utf8_scalar); i32_wrap_i64; local_set(3);
        local_get(0); global_get(parse_pos); call(utf8_width); local_set(5);
        local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_LIT); i32_store(0);
        local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; local_get(3); i32_store(0);
        global_get(parse_pos); local_get(5); i32_add; global_set(parse_pos);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.parse_escape, type_idx, f));
}

/// Allocate `nranges` contiguous Range pairs and write a CLASS node into the
/// Piece at local `piece`, with `negated` (compile-time const). The ranges are
/// supplied via `fill` which stores the lo/hi pairs given the ranges base addr
/// (held in local `rp`). Returns nothing.
fn emit_class_const(
    f: &mut Function, arena_sp: u32, piece: u32, rp: u32, nranges: i32, negated: i32,
    fill: impl Fn(&mut Function, u32),
) {
    emit_node_alloc(f, arena_sp, RX_RANGE_WORDS * nranges);
    wasm!(f, { local_set(rp); });
    fill(f, rp);
    wasm!(f, {
        local_get(piece); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_CLASS); i32_store(0);
        local_get(piece); i32_const(RX_PIECE_X_OFF); i32_add; local_get(rp); i32_store(0);
        local_get(piece); i32_const(RX_PIECE_Y_OFF); i32_add; i32_const(nranges); i32_store(0);
        local_get(piece); i32_const(RX_PIECE_Z_OFF); i32_add; i32_const(negated); i32_store(0);
    });
}

/// Store a const range pair `(lo,hi)` at `rp + index*RANGE_BYTES`.
fn emit_store_range(f: &mut Function, rp: u32, index: i32, lo: i32, hi: i32) {
    let base = index * RX_RANGE_WORDS * RX_WORD;
    wasm!(f, {
        local_get(rp); i32_const(base + RX_RANGE_LO_OFF); i32_add; i32_const(lo); i32_store(0);
        local_get(rp); i32_const(base + RX_RANGE_HI_OFF); i32_add; i32_const(hi); i32_store(0);
    });
}

fn emit_class_digit(f: &mut Function, arena_sp: u32, piece: u32, rp: u32, negated: i32) {
    emit_class_const(f, arena_sp, piece, rp, 1, negated, |f, rp| {
        emit_store_range(f, rp, 0, RX_DIGIT_LO, RX_DIGIT_HI);
    });
}
fn emit_class_word(f: &mut Function, arena_sp: u32, piece: u32, rp: u32, negated: i32) {
    emit_class_const(f, arena_sp, piece, rp, 4, negated, |f, rp| {
        emit_store_range(f, rp, 0, RX_LOWER_LO, RX_LOWER_HI);
        emit_store_range(f, rp, 1, RX_UPPER_LO, RX_UPPER_HI);
        emit_store_range(f, rp, 2, RX_DIGIT_LO, RX_DIGIT_HI);
        emit_store_range(f, rp, 3, RX_UNDERSCORE, RX_UNDERSCORE);
    });
}
fn emit_class_space(f: &mut Function, arena_sp: u32, piece: u32, rp: u32, negated: i32) {
    emit_class_const(f, arena_sp, piece, rp, 4, negated, |f, rp| {
        emit_store_range(f, rp, 0, ASCII_SPACE, ASCII_SPACE);
        emit_store_range(f, rp, 1, ASCII_TAB, ASCII_TAB);
        emit_store_range(f, rp, 2, ASCII_NEWLINE, ASCII_NEWLINE);
        emit_store_range(f, rp, 3, ASCII_CR, ASCII_CR);
    });
}

// ─── __rx_parse_class(pat, piece_ptr) -> () ───
// parse_pos points AFTER '['. Mirrors native rx_parse_class. Ranges are emitted
// contiguously into the arena (nothing else allocates between pairs here), and
// the running base/count are kept in locals; the CLASS node is written at the
// end. Inside the class, `\d \w \s` expand, `\D \n \t` etc. push literals, and
// `a-b` ranges are recognized.
fn compile_parse_class(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.parse_class];
    // params: 0=pat, 1=piece
    // locals: 2=pat_len, 3=neg, 4=ranges_base, 5=nranges, 6=ch, 7=esc, 8=hi,
    //         9=width, 10=lo
    let mut f = Function::new([(9, ValType::I32)]);
    let arena_sp = emitter.rt.regex.arena_sp_global;
    let parse_pos = emitter.rt.regex.parse_pos_global;
    let utf8_scalar = emitter.rt.string.utf8_scalar;
    let utf8_width = emitter.rt.string.utf8_width;
    let data_off = string_data_off();

    wasm!(f, {
        local_get(0); i32_load(0); local_set(2);
        i32_const(0); local_set(3); // neg
        i32_const(0); local_set(5); // nranges
        // leading '^' => negated
        global_get(parse_pos); local_get(2); i32_lt_u;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0);
            i32_const(ASCII_CARET); i32_eq;
            if_empty; i32_const(1); local_set(3); global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); end;
        end;
        // ranges grow contiguously from here
        global_get(arena_sp); local_set(4);
    });

    // Loop: while pos < pat_len && pat[pos] != ']'
    wasm!(f, { block_empty; loop_empty; });
    wasm!(f, { global_get(parse_pos); local_get(2); i32_ge_u; br_if(1); });
    wasm!(f, {
        local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0); local_set(6);
        local_get(6); i32_const(ASCII_RBRACKET); i32_eq; br_if(1);
    });

    // Escape inside class: '\' and pos+1 < pat_len
    wasm!(f, {
        local_get(6); i32_const(ASCII_BACKSLASH); i32_eq;
        global_get(parse_pos); i32_const(1); i32_add; local_get(2); i32_lt_u;
        i32_and;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_const(1); i32_add; i32_add; i32_load8_u(0); local_set(7);
            global_get(parse_pos); i32_const(2); i32_add; global_set(parse_pos);
    });
    // Each `\X` sub-branch sits inside its own `if(escape)` → `if(\X)`; the loop
    // is two structured-control levels out, so `br(2)` continues the class loop.
    // The default branch (no inner `if`) is one level inside `if(escape)`, so it
    // uses `br(1)`.
    // \d
    wasm!(f, { local_get(7); i32_const(ASCII_LOWER_D); i32_eq; if_empty; });
    emit_push_pair_const(&mut f, arena_sp, 4, 5, RX_DIGIT_LO, RX_DIGIT_HI);
    wasm!(f, { br(2); end; });
    // \w
    wasm!(f, { local_get(7); i32_const(ASCII_LOWER_W); i32_eq; if_empty; });
    emit_push_pair_const(&mut f, arena_sp, 4, 5, RX_LOWER_LO, RX_LOWER_HI);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, RX_UPPER_LO, RX_UPPER_HI);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, RX_DIGIT_LO, RX_DIGIT_HI);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, RX_UNDERSCORE, RX_UNDERSCORE);
    wasm!(f, { br(2); end; });
    // \s
    wasm!(f, { local_get(7); i32_const(ASCII_LOWER_S); i32_eq; if_empty; });
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_SPACE, ASCII_SPACE);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_TAB, ASCII_TAB);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_NEWLINE, ASCII_NEWLINE);
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_CR, ASCII_CR);
    wasm!(f, { br(2); end; });
    // \n
    wasm!(f, { local_get(7); i32_const(ASCII_LOWER_N); i32_eq; if_empty; });
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_NEWLINE, ASCII_NEWLINE);
    wasm!(f, { br(2); end; });
    // \t
    wasm!(f, { local_get(7); i32_const(ASCII_LOWER_T); i32_eq; if_empty; });
    emit_push_pair_const(&mut f, arena_sp, 4, 5, ASCII_TAB, ASCII_TAB);
    wasm!(f, { br(2); end; });
    // default (incl \D \S \W \r): literal esc byte → (esc, esc)
    emit_push_pair_local(&mut f, arena_sp, 4, 5, 7, 7);
    wasm!(f, { br(1); });
    wasm!(f, { end; }); // end escape branch

    // Non-escape: decode scalar (lo), advance by width
    wasm!(f, {
        local_get(0); global_get(parse_pos); call(utf8_scalar); i32_wrap_i64; local_set(10);
        local_get(0); global_get(parse_pos); call(utf8_width); local_set(9);
        global_get(parse_pos); local_get(9); i32_add; global_set(parse_pos);
    });
    // range a-b: '-' at pos && byte after '-' exists && != ']'
    wasm!(f, {
        global_get(parse_pos); local_get(2); i32_lt_u;
        if_empty;
            local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_add; i32_load8_u(0);
            i32_const(ASCII_DASH); i32_eq;
            global_get(parse_pos); i32_const(1); i32_add; local_get(2); i32_lt_u;
            i32_and;
            if_empty;
                local_get(0); i32_const(data_off); i32_add; global_get(parse_pos); i32_const(1); i32_add; i32_add; i32_load8_u(0);
                i32_const(ASCII_RBRACKET); i32_ne;
                if_empty;
                    // consume '-', decode end scalar (hi)
                    global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos);
                    local_get(0); global_get(parse_pos); call(utf8_scalar); i32_wrap_i64; local_set(8);
                    local_get(0); global_get(parse_pos); call(utf8_width); local_set(9);
                    global_get(parse_pos); local_get(9); i32_add; global_set(parse_pos);
    });
    emit_push_pair_local(&mut f, arena_sp, 4, 5, 10, 8); // (lo, hi)
    wasm!(f, {
                    // 3 nested `if`s deep (if(!]) / if(dash) / if(pos<len)); the
                    // loop is the 4th level out, so `br(3)` continues the loop.
                    br(3);
                end;
            end;
        end;
    });
    // single char: (lo, lo)
    emit_push_pair_local(&mut f, arena_sp, 4, 5, 10, 10);
    wasm!(f, { br(0); });
    wasm!(f, { end; end; }); // end loop, block

    // skip closing ']'
    wasm!(f, {
        global_get(parse_pos); local_get(2); i32_lt_u;
        if_empty; global_get(parse_pos); i32_const(1); i32_add; global_set(parse_pos); end;
    });
    // write CLASS node
    wasm!(f, {
        local_get(1); i32_const(RX_PIECE_KIND_OFF); i32_add; i32_const(RX_KIND_CLASS); i32_store(0);
        local_get(1); i32_const(RX_PIECE_X_OFF); i32_add; local_get(4); i32_store(0);
        local_get(1); i32_const(RX_PIECE_Y_OFF); i32_add; local_get(5); i32_store(0);
        local_get(1); i32_const(RX_PIECE_Z_OFF); i32_add; local_get(3); i32_store(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.parse_class, type_idx, f));
}

/// Push a const range pair onto the contiguous ranges array (bump arena one
/// pair, store lo/hi, nranges++). Used by `\d \w \s` expansion inside a class.
fn emit_push_pair_const(f: &mut Function, arena_sp: u32, ranges_base: u32, nranges: u32, lo: i32, hi: i32) {
    emit_node_alloc(f, arena_sp, RX_RANGE_WORDS);
    wasm!(f, { drop; }); // base already known via ranges_base + nranges
    wasm!(f, {
        local_get(ranges_base); local_get(nranges); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add;
        i32_const(lo); i32_store(RX_RANGE_LO_OFF);
        local_get(ranges_base); local_get(nranges); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add;
        i32_const(hi); i32_store(RX_RANGE_HI_OFF);
        local_get(nranges); i32_const(1); i32_add; local_set(nranges);
    });
}

/// Push a range pair whose lo/hi come from locals `lo_l`/`hi_l`.
fn emit_push_pair_local(f: &mut Function, arena_sp: u32, ranges_base: u32, nranges: u32, lo_l: u32, hi_l: u32) {
    emit_node_alloc(f, arena_sp, RX_RANGE_WORDS);
    wasm!(f, { drop; });
    wasm!(f, {
        local_get(ranges_base); local_get(nranges); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add;
        local_get(lo_l); i32_store(RX_RANGE_LO_OFF);
        local_get(ranges_base); local_get(nranges); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add;
        local_get(hi_l); i32_store(RX_RANGE_HI_OFF);
        local_get(nranges); i32_const(1); i32_add; local_set(nranges);
    });
}

// ════════════════════════════════════════════════════════════════════════
//  Matcher
// ════════════════════════════════════════════════════════════════════════

// ─── __rx_node_matches(piece_ptr, scalar) -> 0/1 ───
// Mirrors native rx_node_matches: Lit==scalar, Dot=scalar!='\n', Class=ranges.
fn compile_node_matches(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.node_matches];
    // params: 0=piece, 1=scalar
    // locals: 2=kind, 3=ranges, 4=nranges, 5=neg, 6=i, 7=hit, 8=lo, 9=hi
    let mut f = Function::new([(8, ValType::I32)]);
    wasm!(f, {
        local_get(0); i32_load(0); local_set(2); // kind
        // Lit
        local_get(2); i32_const(RX_KIND_LIT); i32_eq;
        if_empty;
            local_get(1); local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); i32_eq;
            return_;
        end;
        // Dot
        local_get(2); i32_const(RX_KIND_DOT); i32_eq;
        if_empty;
            local_get(1); i32_const(ASCII_NEWLINE); i32_ne;
            return_;
        end;
        // Class
        local_get(2); i32_const(RX_KIND_CLASS); i32_eq;
        if_empty;
            local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); local_set(3);
            local_get(0); i32_const(RX_PIECE_Y_OFF); i32_add; i32_load(0); local_set(4);
            local_get(0); i32_const(RX_PIECE_Z_OFF); i32_add; i32_load(0); local_set(5);
            i32_const(0); local_set(7); // hit
            i32_const(0); local_set(6); // i
            block_empty; loop_empty;
                local_get(6); local_get(4); i32_ge_u; br_if(1);
                local_get(3); local_get(6); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add; i32_load(RX_RANGE_LO_OFF); local_set(8);
                local_get(3); local_get(6); i32_const(RX_RANGE_WORDS * RX_WORD); i32_mul; i32_add; i32_load(RX_RANGE_HI_OFF); local_set(9);
                // lo <= scalar <= hi (signed; scalars are non-negative)
                local_get(1); local_get(8); i32_ge_s;
                local_get(1); local_get(9); i32_le_s;
                i32_and;
                if_empty; i32_const(1); local_set(7); br(2); end;
                local_get(6); i32_const(1); i32_add; local_set(6);
                br(0);
            end; end;
            // hit XOR neg
            local_get(7); local_get(5); i32_ne;
            return_;
        end;
        // anchors / group never reach node_matches
        i32_const(0);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.node_matches, type_idx, f));
}

// ─── __rx_match_one(piece_ptr, text, p, caps, ncap) -> bytes_consumed | -1 ───
// p is a BYTE offset. Returns BYTES consumed (caller advances p+ret) or -1.
// Mirrors native rx_match_one (native returns char count; we return bytes).
fn compile_match_one(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_one];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=kind, 6=text_len, 7=scalar, 8=width, 9=inner, 10=end, 11=ci, 12=start
    let mut f = Function::new([(8, ValType::I32)]);
    let node_matches = emitter.rt.regex.node_matches;
    let match_alts = emitter.rt.regex.match_alts;
    let utf8_scalar = emitter.rt.string.utf8_scalar;
    let utf8_width = emitter.rt.string.utf8_width;

    wasm!(f, {
        local_get(0); i32_load(0); local_set(5); // kind
        local_get(1); i32_load(0); local_set(6); // text_len bytes
    });
    // Group
    wasm!(f, {
        local_get(5); i32_const(RX_KIND_GROUP); i32_eq;
        if_empty;
            local_get(2); local_set(12); // start = p
            local_get(0); i32_const(RX_PIECE_X_OFF); i32_add; i32_load(0); local_set(9); // inner alts
            local_get(9); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_alts);
            local_set(10);
            local_get(10); i32_const(RX_NO_MATCH); i32_eq;
            if_empty; i32_const(RX_NO_MATCH); return_; end;
            // caps[ci-1] = (start,end) when ci>0
            local_get(0); i32_const(RX_PIECE_Z_OFF); i32_add; i32_load(0); local_set(11);
            local_get(11); i32_const(0); i32_gt_u;
            if_empty;
                local_get(3); local_get(11); i32_const(1); i32_sub; i32_const(RX_CAP_BYTES); i32_mul; i32_add;
                local_get(12); i32_store(RX_CAP_START_OFF);
                local_get(3); local_get(11); i32_const(1); i32_sub; i32_const(RX_CAP_BYTES); i32_mul; i32_add;
                local_get(10); i32_store(RX_CAP_END_OFF);
            end;
            local_get(10); local_get(2); i32_sub; return_; // bytes consumed
        end;
    });
    // Lit / Dot / Class: one scalar at p
    wasm!(f, {
        local_get(2); local_get(6); i32_ge_u;
        if_empty; i32_const(RX_NO_MATCH); return_; end;
        local_get(1); local_get(2); call(utf8_scalar); i32_wrap_i64; local_set(7);
        local_get(0); local_get(7); call(node_matches);
        if_empty;
            local_get(1); local_get(2); call(utf8_width); local_set(8);
            local_get(8); return_;
        else_;
            i32_const(RX_NO_MATCH); return_;
        end;
    });
    wasm!(f, { i32_const(RX_NO_MATCH); end; });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_one, type_idx, f));
}

// ─── __rx_match_rep(piece, text, p, caps, ncap, count) -> end | -1 ───
// Greedy repetition over a single Piece, then its `next`. Mirrors native
// rx_match_rep with caps save/restore on the greedy branch.
fn compile_match_rep(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_rep];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap, 5=count
    // locals: 6=max, 7=min, 8=at_max, 9=consumed, 10=res, 11=save, 12=next
    let mut f = Function::new([(7, ValType::I32)]);
    let match_one = emitter.rt.regex.match_one;
    let match_seq = emitter.rt.regex.match_seq;
    let match_rep = emitter.rt.regex.match_rep;
    let save_sp = emitter.rt.regex.save_sp_global;

    wasm!(f, {
        local_get(0); i32_const(RX_PIECE_MAX_OFF); i32_add; i32_load(0); local_set(6);
        local_get(0); i32_const(RX_PIECE_MIN_OFF); i32_add; i32_load(0); local_set(7);
        // at_max = max != UNBOUNDED && count >= max
        i32_const(0); local_set(8);
        local_get(6); i32_const(RX_MAX_UNBOUNDED); i32_ne;
        if_empty; local_get(5); local_get(6); i32_ge_u; local_set(8); end;
    });
    // greedy: if !at_max try one more
    wasm!(f, { local_get(8); i32_eqz; if_empty; });
    emit_caps_push(&mut f, save_sp, 3, 4, 11);
    wasm!(f, {
            local_get(0); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_one);
            local_set(9);
            local_get(9); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
                // zero-width guard: consumed>0 || count==0
                local_get(9); i32_const(0); i32_gt_u;
                local_get(5); i32_eqz;
                i32_or;
                if_empty;
                    local_get(0); local_get(1);
                    local_get(2); local_get(9); i32_add; // p+consumed
                    local_get(3); local_get(4);
                    local_get(5); i32_const(1); i32_add; // count+1
                    call(match_rep);
                    local_set(10);
                    local_get(10); i32_const(RX_NO_MATCH); i32_ne;
                    if_empty;
    });
    emit_caps_pop_discard(&mut f, save_sp, 11);
    wasm!(f, {
                        local_get(10); return_;
                    end;
                end;
            end;
    });
    emit_caps_restore(&mut f, save_sp, 3, 4, 11);
    wasm!(f, { end; }); // end !at_max
    // if count >= min: match rest of seq (piece.next)
    wasm!(f, {
        local_get(5); local_get(7); i32_ge_u;
        if_empty;
            local_get(0); i32_const(RX_PIECE_NEXT_OFF); i32_add; i32_load(0); local_set(12);
            local_get(12); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_seq);
            return_;
        end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_rep, type_idx, f));
}

// ─── __rx_match_seq(piece, text, p, caps, ncap) -> end | -1 ───
// Match the linked piece list starting at `piece` (null = success). Anchors are
// handled here; everything else delegates to rep. Mirrors native rx_match_seq.
fn compile_match_seq(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_seq];
    // params: 0=piece, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=kind, 6=text_len, 7=next
    let mut f = Function::new([(3, ValType::I32)]);
    let match_seq = emitter.rt.regex.match_seq;
    let match_rep = emitter.rt.regex.match_rep;

    wasm!(f, {
        // piece == null => success (return p)
        local_get(0); i32_eqz;
        if_empty; local_get(2); return_; end;
        local_get(0); i32_load(0); local_set(5); // kind
        local_get(1); i32_load(0); local_set(6); // text_len bytes
        local_get(0); i32_const(RX_PIECE_NEXT_OFF); i32_add; i32_load(0); local_set(7); // next
        // AnchorStart: p==0
        local_get(5); i32_const(RX_KIND_ANCHOR_START); i32_eq;
        if_empty;
            local_get(2); i32_eqz;
            if_empty;
                local_get(7); local_get(1); local_get(2); local_get(3); local_get(4);
                call(match_seq); return_;
            else_;
                i32_const(RX_NO_MATCH); return_;
            end;
        end;
        // AnchorEnd: p==text_len(bytes)
        local_get(5); i32_const(RX_KIND_ANCHOR_END); i32_eq;
        if_empty;
            local_get(2); local_get(6); i32_eq;
            if_empty;
                local_get(7); local_get(1); local_get(2); local_get(3); local_get(4);
                call(match_seq); return_;
            else_;
                i32_const(RX_NO_MATCH); return_;
            end;
        end;
        // else: rep over this piece, count=0
        local_get(0); local_get(1); local_get(2); local_get(3); local_get(4); i32_const(0);
        call(match_rep);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_seq, type_idx, f));
}

// ─── __rx_match_alts(alts, text, p, caps, ncap) -> end | -1 ───
// Try each Seq alternative; save/restore caps. Mirrors native rx_match_alts.
fn compile_match_alts(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.match_alts];
    // params: 0=alts, 1=text, 2=p, 3=caps, 4=ncap
    // locals: 5=seq, 6=res, 7=save, 8=head_piece
    let mut f = Function::new([(4, ValType::I32)]);
    let match_seq = emitter.rt.regex.match_seq;
    let save_sp = emitter.rt.regex.save_sp_global;

    wasm!(f, {
        local_get(0); i32_const(RX_ALTS_HEAD_OFF); i32_add; i32_load(0); local_set(5); // first seq
        block_empty; loop_empty;
            local_get(5); i32_eqz; br_if(1); // no more alts → fail
            local_get(5); i32_const(RX_SEQ_HEAD_OFF); i32_add; i32_load(0); local_set(8); // head piece
    });
    emit_caps_push(&mut f, save_sp, 3, 4, 7);
    wasm!(f, {
            local_get(8); local_get(1); local_get(2); local_get(3); local_get(4);
            call(match_seq);
            local_set(6);
            local_get(6); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
    });
    emit_caps_pop_discard(&mut f, save_sp, 7);
    wasm!(f, {
                local_get(6); return_;
            end;
    });
    emit_caps_restore(&mut f, save_sp, 3, 4, 7);
    wasm!(f, {
            local_get(5); i32_const(RX_SEQ_NEXT_OFF); i32_add; i32_load(0); local_set(5); // next alt
            br(0);
        end; end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.match_alts, type_idx, f));
}

// ─── __rx_find_at(alts, text, start, caps, ncap) -> end | -1 ───
// Search from byte offset `start`; mirrors native rx_find_at (resets caps to
// unset on each position, sets match_start_global). `start` advances by scalar
// width so byte offsets stay codepoint-aligned. The search range is
// `start..=byte_len` (i may equal len for zero-width / anchor matches).
fn compile_find_at(emitter: &mut WasmEmitter) {
    let type_idx = emitter.func_type_indices[&emitter.rt.regex.find_at];
    // params: 0=alts, 1=text, 2=start, 3=caps, 4=ncap
    // locals: 5=text_len, 6=i, 7=res, 8=k, 9=width
    let mut f = Function::new([(5, ValType::I32)]);
    let match_alts = emitter.rt.regex.match_alts;
    let match_start = emitter.rt.regex.match_start_global;
    let utf8_width = emitter.rt.string.utf8_width;

    wasm!(f, {
        local_get(1); i32_load(0); local_set(5); // text_len bytes
        local_get(2); local_set(6); // i = start
        block_empty; loop_empty;
            local_get(6); local_get(5); i32_gt_u; br_if(1); // i > len → stop
    });
    emit_caps_reset(&mut f, 3, 4, 8);
    wasm!(f, {
            local_get(0); local_get(1); local_get(6); local_get(3); local_get(4);
            call(match_alts);
            local_set(7);
            local_get(7); i32_const(RX_NO_MATCH); i32_ne;
            if_empty;
                local_get(6); global_set(match_start);
                local_get(7); return_;
            end;
            // advance i by one scalar width; if i==len, no more positions.
            local_get(6); local_get(5); i32_ge_u;
            if_empty; i32_const(RX_NO_MATCH); return_; end;
            local_get(1); local_get(6); call(utf8_width); local_set(9);
            local_get(6); local_get(9); i32_add; local_set(6);
            br(0);
        end; end;
        i32_const(RX_NO_MATCH);
        end;
    });
    emitter.add_compiled(CompiledFunc::tracked_for(emitter.rt.regex.find_at, type_idx, f));
}

// ════════════════════════════════════════════════════════════════════════
//  Capture save-stack helpers (bump push/pop of ncap × 2 words)
// ════════════════════════════════════════════════════════════════════════

/// Push a copy of `caps` (ncap slots) onto the save stack; record the saved
/// base into `save_local`. Bumps `save_sp` by ncap × RX_CAP_BYTES.
fn emit_caps_push(f: &mut Function, save_sp: u32, caps: u32, ncap: u32, save_local: u32) {
    wasm!(f, {
        global_get(save_sp); local_set(save_local);
        global_get(save_sp); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul; i32_add; global_set(save_sp);
        // memory_copy(dst=save_base, src=caps, n=ncap*RX_CAP_BYTES)
        local_get(save_local); local_get(caps); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul;
        memory_copy;
    });
}

/// Restore `caps` from the saved copy at `save_local`, then pop (rewind save_sp).
fn emit_caps_restore(f: &mut Function, save_sp: u32, caps: u32, ncap: u32, save_local: u32) {
    wasm!(f, {
        local_get(caps); local_get(save_local); local_get(ncap); i32_const(RX_CAP_BYTES); i32_mul;
        memory_copy;
        local_get(save_local); global_set(save_sp);
    });
}

/// Discard the saved copy at `save_local` (success path): rewind save_sp.
fn emit_caps_pop_discard(f: &mut Function, save_sp: u32, save_local: u32) {
    wasm!(f, { local_get(save_local); global_set(save_sp); });
}

/// Reset all caps slots to UNSET (-1). `i` is a scratch loop local.
fn emit_caps_reset(f: &mut Function, caps: u32, ncap: u32, i: u32) {
    wasm!(f, {
        i32_const(0); local_set(i);
        block_empty; loop_empty;
            local_get(i); local_get(ncap); i32_ge_u; br_if(1);
            local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add;
            i32_const(RX_CAP_UNSET); i32_store(RX_CAP_START_OFF);
            local_get(caps); local_get(i); i32_const(RX_CAP_BYTES); i32_mul; i32_add;
            i32_const(RX_CAP_UNSET); i32_store(RX_CAP_END_OFF);
            local_get(i); i32_const(1); i32_add; local_set(i);
            br(0);
        end; end;
    });
}
