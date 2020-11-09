use lazy_id::Id;

#[test]
fn test_eq() {
    assert_ne!(Id::new(), Id::new());
    let v = Id::new();
    assert_eq!(v.clone(), v);
    assert_eq!(v, v.get());
    assert_eq!(v.get(), v);
}

#[test]
fn test_cmp() {
    use std::collections::BTreeMap;
    let ids: Vec<Id> = (0..100).map(|_| Id::lazy()).collect();
    let map: BTreeMap<Id, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, v)| (v.clone(), i))
        .collect();
    assert_eq!(map[&ids[10]], 10);
    let idv = ids[20].get();
    assert_eq!(map.get(&idv), Some(&20usize));
    assert_eq!(map.get(&Id::new()), None);
    use core::cmp::Ordering::Equal;
    assert!(!matches!(
        Id::new().partial_cmp(&Id::new()),
        None | Some(Equal)
    ));
}

#[test]
fn test_hash() {
    use std::collections::HashMap;
    let ids: Vec<Id> = (0..100).map(|_| Id::lazy()).collect();
    let map: HashMap<Id, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, v)| (v.clone(), i))
        .collect();
    assert_eq!(map[&ids[10]], 10);
    let idv = ids[20].get();
    assert_eq!(map.get(&idv), Some(&20usize));
    assert_eq!(map.get(&Id::new()), None);
}

#[test]
fn test_fmt() {
    let lazy = Id::lazy();
    let id2seq = 0x1337_fe4415;
    assert_eq!(lazy.to_string(), lazy.get().to_string());
    // this mostly makes sure the seq is right.
    let expect = format!(
        "Id({:#x}; seq={})",
        lazy.get(),
        lazy.get().wrapping_mul(id2seq)
    );
    assert_eq!(format!("{:?}", lazy), expect);
}

#[test]
fn test_convert() {
    let id = Id::new();
    {
        let r = id.as_ref();
        assert_eq!(&id.get(), r);
    }
    let v = id.get();
    assert_eq!(v, u64::from(&id));
    assert_eq!(v, u64::from(id.clone()));
    let vnz = id.get_nonzero();
    assert_eq!(vnz.get(), v);
    assert_eq!(vnz, core::num::NonZeroU64::from(id.clone()));
}
