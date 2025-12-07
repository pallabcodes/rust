use fundamentals::{
    async_primer_demo, closures_demo, concurrency_primitives_demo, enums_demo, lifetimes_demo,
    smart_pointers_demo, traits_demo, variables_demo,
};

#[test]
fn basics_render_output() {
    let out = variables_demo();
    assert!(!out.is_empty());
}

#[test]
fn lifetimes_and_traits_render() {
    let out_life = lifetimes_demo();
    let out_trait = traits_demo();
    assert!(out_life.contains("longer"));
    assert!(out_trait.contains("Point"));
}

#[test]
fn closures_cover_modes() {
    let out = closures_demo();
    assert!(out.contains("FnOnce") && out.contains("FnMut"));
}

#[test]
fn enums_cover_variants() {
    let out = enums_demo();
    assert!(out.contains("pending") && out.contains("shipped"));
}

#[test]
fn smart_pointers_cover_shared() {
    let out = smart_pointers_demo();
    assert!(out.contains("Arc<Mutex>"));
}

#[test]
fn concurrency_and_async_render() {
    let conc = concurrency_primitives_demo();
    let async_out = async_primer_demo();
    assert!(conc.contains("RwLock") && async_out.contains("abort"));
}

