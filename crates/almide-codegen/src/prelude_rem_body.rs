/// The magic multiplier and shift for a signed division by the constant
/// `d >= 2` (Hacker's Delight, 2nd ed., figure 10-1, for d > 0).
pub const fn almide_rem_magic(d: i64) -> (i64, u32) {
    let two63: u64 = 1 << 63;
    let ad = d as u64;
    let anc = two63 - 1 - two63 % ad;
    let mut p: u32 = 63;
    let mut q1 = two63 / anc;
    let mut r1 = two63 - q1 * anc;
    let mut q2 = two63 / ad;
    let mut r2 = two63 - q2 * ad;
    loop {
        p += 1;
        q1 = q1.wrapping_mul(2);
        r1 = r1.wrapping_mul(2);
        if r1 >= anc {
            q1 = q1.wrapping_add(1);
            r1 = r1.wrapping_sub(anc);
        }
        q2 = q2.wrapping_mul(2);
        r2 = r2.wrapping_mul(2);
        if r2 >= ad {
            q2 = q2.wrapping_add(1);
            r2 = r2.wrapping_sub(ad);
        }
        let delta = ad - r2;
        if !(q1 < delta || (q1 == delta && r1 == 0)) {
            break;
        }
    }
    (q2.wrapping_add(1) as i64, p - 64)
}

pub struct AlmideRemConst<const D: i64>;

// `{ D }`, not `D`: a generic argument is read as a TYPE first, so a user
// record named `D` in the same crate made this `E0747` on every build that
// inlines the runtime (the cargo path: cross builds, matrix programs, native
// deps). Found by the Miri harness, which builds that way (#4009).
impl<const D: i64> AlmideRemConst<{ D }> {
    pub const MAGIC: (i64, u32) = almide_rem_magic(D);
}

/// The high 64 bits of the signed product `a * b`. On x86_64 this is the
/// one-operand `imul`: with BMI2 enabled (the native build's default
/// `x86-64-v3`) LLVM lowers the i128 form to an unsigned `mulx` plus sign
/// fix-ups, a longer chain than the signed multiply it stands for.
#[inline(always)]
pub fn almide_mulhs(a: i64, b: i64) -> i64 {
    // Miri cannot run inline assembly, so under Miri the portable form runs
    // on x86_64 too (#4009): the nightly Miri job is on x86_64 Linux.
    #[cfg(all(target_arch = "x86_64", not(miri)))]
    {
        let hi: i64;
        // SAFETY: `imul r64` reads rax and the operand, writes rdx:rax and
        // the flags, and touches no memory.
        unsafe {
            std::arch::asm!("imul {b}", b = in(reg) b, inout("rax") a => _, out("rdx") hi, options(pure, nomem, nostack));
        }
        hi
    }
    #[cfg(any(not(target_arch = "x86_64"), miri))]
    {
        ((a as i128 * b as i128) >> 64) as i64
    }
}

/// `x % D` for a literal divisor `D >= 3` that is not a power of two: the
/// same truncated remainder as `%`, never aborting. The quotient is the
/// magic-number estimate, and the dividend's sign corrects the remainder
/// rather than the quotient, so the correction is computed beside the
/// multiply instead of after it.
#[inline(always)]
pub fn almide_rem_const<const D: i64>(x: i64) -> i64 {
    let (m, s) = AlmideRemConst::<{ D }>::MAGIC;
    let hi = almide_mulhs(x, m);
    let t = if m < 0 { hi.wrapping_add(x) } else { hi };
    x.wrapping_sub((x >> 63) & D).wrapping_sub((t >> s).wrapping_mul(D))
}
