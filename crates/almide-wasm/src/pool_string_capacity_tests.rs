use crate::func::Pool;

#[test]
fn pooled_strings_have_no_spare_capacity() {
    let mut pool = Pool::new();
    for n in 0..=1024 {
        for ch in ['a', 'é', '😀'] {
            let s: String = std::iter::repeat_n(ch, n).collect();
            let base = pool.intern(&s) as usize;
            let read = |offset: u32| {
                u32::from_le_bytes(pool.data[base + offset as usize..][..4].try_into().expect("a four-byte slice"))
            };
            assert_eq!(read(almide_layout::LEN.offset), s.len() as u32);
            assert_eq!(
                read(almide_layout::CAP.offset),
                read(almide_layout::LEN.offset)
            );
            assert_eq!(pool.intern(&s) as usize, base);
        }
    }
}
