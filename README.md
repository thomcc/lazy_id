# `lazy_id`
[![Build Status](https://github.com/thomcc/lazy_id/workflows/CI/badge.svg)](https://github.com/thomcc/lazy_id/actions)
[![codecov](https://codecov.io/gh/thomcc/lazy_id/branch/main/graph/badge.svg)](https://codecov.io/gh/thomcc/lazy_id)
[![Docs](https://docs.rs/lazy_id/badge.svg)](https://docs.rs/lazy_id)
[![Latest Version](https://img.shields.io/crates/v/lazy_id.svg)](https://crates.io/crates/lazy_id)

Provides `lazy_id::Id`, a thread-safe 64-bit id that only initializes itself to a specific value when you use it rather than when you create it. It works with `no_std` (without `liballoc` either), is entirely lock-free, currently supports versions as far back as 1.34.0, and has zero dependencies other than libcore.

## Usage

```rust
use lazy_id::Id;
struct Thing {
    id: Id,
    // other fields, ...
}
// Now this function can be const, allowing use in statics
const fn new_thing() -> Thing {
    Thing { id: Id::lazy(), /* ... */ }
}
static C: Thing = new_thing();
// also works for non-static without meaningful overhead
let a = new_thing();
let b = new_thing();
// `Id` implements `PartialEq`, and many other useful traits.
assert!(a.id != b.id && a.id != C.id);
```

## Why would this be helpful?

If you need a unique per-instance `Id` for your type, usually the approach is a global atomic that you increment each time you allocate an id.

The only problem here is that now if you want to store your type in a `static` of some sort, you need to use [`OnceCell`](https://crates.io/crates/once_cell), [`lazy_static`](https://crates.io/crates/lazy_static), or some other lazy initialization crate. These crates are fine and wouldn't be expected to cause perfomance problems the majority of the time, but they can be frustrating if you didn't already need them, and can be extremely undesirable to force this on users of your library.

They also are a dependency you want to avoid if previously you were `no_std` — You essentially need to hold a lock while doing generic thread-safe lazy init, and doing that with no_std requires a spinlock, which is... not great (this is why `once_cell` requires std for the `sync` functionality).

So, why doesn't `lazy_id` have this problem? Well, we're not doing any sort of generic initialization. Ours is extremely specific and concrete, and was designed to avoid taking a lock. This means we can easily avoid the `std` dep without any spinning.

