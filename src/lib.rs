//! [`lazy_id::Id`](Id) is a thread-safe 64-bit id that only initializes itself
//! to a specific value when you use it rather than when you create it.
//!
//! Why would this be helpful? If you need a unique per-instance `Id` for your
//! type, usually the approach is a global atomic that you increment each time
//! you allocate an id.  The only problem here is that now if you want to store
//! your type in a `static` of some sort, you need to use `OnceCell` or
//! `lazy_static`.
//!
//! This can be pretty annoying if your API is most useful as a `static`, and
//! you've been carefully designing to allow static initialization, especially
//! this now means the API you designed forces users to use these libraries
//! around your types.
//!
//! In my case, I was playing around with low level threading code already, and
//! the extra locking these imposed on all accesses felt like it completely
//! defeated the point of my fancy data structure, and would have shown up in
//! the public api.
//!
//! Anyway, unlike `lazy_static`/`OnceCell`/the hypothetical `std::lazy`, this
//! crate is entirely lock free, and only uses a few relaxed atomic operations
//! to initialize itself on first access (automatically), and the fast path of
//! reading an already-initialized `Id` is just a single relaxed `load`. This is
//! all to say, it's much more efficient than most of the alternatives would be
//! and more efficient than I had expected it to be.
#![no_std]
use core::num::NonZeroU64;
use core::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// A thread-safe lazily-initialized 64-bit ID.
///
/// This is useful if you have a structure which needs a unique ID, but don't
/// want to return it from a const fn, or for callers to be able to use it in
/// statics, without requiring that they lazily initialize the whole thing, e.g.
/// via `lazy_static` / `OnceCell` / etc.
///
/// The `Id` type initializes exactly once, and never returns a duplicate — the
/// only `Id`s which might have the same value as others are ones that come from
/// `Id`'s impl of `Clone` or [`Id::from_raw_integer`].
///
/// It supports most traits you'd want, including `Hash`, `Ord`, `Clone`,
/// `Deref<Target = u64>`, `PartialEq<u64>` (and `u64` has `PartialEq<Id>`),
/// `Debug`, `Display`, `Default` (same as [`Id::new`])...
///
/// `Id`'s initialization is entirely lock-free and uses only relaxed atomic
/// operations (nor is anything stronger needed). The fast path of [`Id::get`]
/// is just a `Relaxed` atomic load, which is the same cost as a non-atomic load
/// (on platforms that support 64-bit atomics efficiently, anyway).
///
/// # Example
///
/// ```
/// use lazy_id::Id;
/// struct Thing {
///     id: Id,
///     // other fields, ...
/// }
/// // Now this function can be const, which can let
/// // callers avoid needing to use `lazy_static`/`OnceCell`
/// // for constants of your type
/// const fn new_thing() -> Thing {
///     Thing { id: Id::lazy(), /* ... */ }
/// }
/// static C: Thing = new_thing();
/// let a = new_thing();
/// let b = new_thing();
/// assert!(a.id != b.id && a.id != C.id);
/// ```
///
/// ## FAQs
///
/// (Okay, nobody's asked me any of these, but it's a good format for misc.
/// documentation notes).
///
/// ### Are `Id`s unique?
///
/// Across different runs of your program? No. Id generation order is
/// deterministic and occurs in the same order every time.
///
/// Within a single run of your program? Yes, with two caveats:
///
/// 1. `Id` implements `Clone` by producing other `Id`s with the same numeric
///    value. This seems desirable, as it makes the `Id` behave as if it had
///    been produced eagerly, and more like a normal number.
///
/// 2. The function [`Id::from_raw_integer`] forces the creation of an `Id` with
///    a specific numeric value, which may or may not be a value which has been
///    returned already, and may or may not be one we'll return in the future.
///    This function should be used with care.
///
/// It's intentionally okay for unsafe code to assume `Id`s that it creates
/// through [`Id::new`]/[`Id::lazy`]/[`Id::LAZY_INITIALIZER`] will all have
/// distinct values.
///
/// ### You mentioned a counter, what about overflow?
///
/// The counter is 64 bits, so this will realistically never happen. If we
/// assume [`Id::new`] takes 1ns (optimistic), this would take 292 *years*.
/// Attempting to bring this down with ✨The Power Of Fearless Concurrency✨ would
/// probably not change this much (or would make it slower), due to that
/// increasing contention on the counter, but who knows.
///
/// If we do overflow, we `abort`. The `abort` (and not `panic`) is because it
/// is global state that is compromised, and so all threads need to be brought
/// down. Additionally, I want `lazy_id::Id` usable in cases where unsafe code
/// can rely on the values returned being unique (so long as they can ensure
/// that none of them came from `Id::from_raw_integer`).
///
/// ### What is `seq=` in the `"{:?}"` output of an `Id`?
///
/// Id debug formats like `"Id(0xhexhexhex; seq=32)"`. The `seq` value is a
/// monotonically increasing value that can help identify the order `Id`s were
/// initialized in, but mostly is a vastly more readable number than the real
/// number, which makes it good for debug output.
///
/// I may expose a way to convert between `id` values and `seq` values in the
/// future, let me know if you need it.
///
/// For a little more explanation: By default, ids are mixed somewhat, which
/// helps discourage people from using them as indexes into arrays or assuming
/// they're sequential, etc (they aren't — they're just monotonic). It also
/// might help them be better hash keys, but with a good hash algo it won't
/// matter.
#[repr(transparent)]
pub struct Id(AtomicU64);

impl Id {
    /// Create an `Id` that will be automatically assigned a value when it's
    /// needed.
    ///
    /// ```
    /// use lazy_id::Id;
    /// struct Thing {
    ///     id: Id,
    ///     // other fields, ...
    /// }
    /// // Now this function can be const, which can let
    /// // callers avoid needing to use `lazy_static`/`OnceCell`
    /// // for constants of your type
    /// const fn new_thing() -> Thing {
    ///     Thing { id: Id::lazy(), /* ... */ }
    /// }
    /// static C: Thing = new_thing();
    /// let a = new_thing();
    /// let b = new_thing();
    /// assert!(a.id != b.id && a.id != C.id);
    /// ```
    ///
    /// If you are not in a const context or other situation where you need to
    /// use lazy initialization, [`Id::new`] is a little more efficient.
    ///
    /// If you're in an array literal initializer, [`Self::LAZY_INITIALIZER`] may
    /// work better for you. Note that using any of these `vec!` literal will
    /// produce a vector with `n` clones of the same `Id`, as it invokes
    /// `clone()` — e.g. `vec![Id::lazy(); n]` should probably be written as,
    /// `(0..n).map(|_| Id::lazy()).collect::<Vec<_>>()`, which will do the
    /// right thing (Note that because this isn't const, using `Id::new()` in
    /// the `map` function would be even better, but isn't the point). This is a
    /// problem inherent with `vec!`, and other types have it as well.
    #[inline]
    pub const fn lazy() -> Self {
        Self::LAZY_INITIALIZER
    }

    /// Create an `Id` which has been initialized eagerly.
    ///
    /// When you don't need the `const`, use this, as it is more efficient.
    ///
    /// See [`Id::lazy`] for the lazy-init version, which is the main selling
    /// point of this crate.
    /// # Example
    /// ```
    /// # use lazy_id::Id;
    /// let a = Id::new();
    /// let b = Id::new();
    ///
    /// assert_ne!(a, b);
    /// ```
    #[inline]
    pub fn new() -> Self {
        Self(AtomicU64::new(Self::next_id().get()))
    }

    /// Equivalent to [`Id::lazy()`](Id::lazy) but usable in situations like
    /// static array initializers (or non-static ones too).
    ///
    /// For example, the fails because `Id` isn't `Copy`, and even if it worked
    /// for `clone()`, it would produce the wrong value.
    ///
    /// ```compile_fail
    /// # use lazy_id::Id;
    /// // Doesn't work :(
    /// static ARR: [Id; 2] = [Id::lazy(); 2];
    /// ```
    ///
    /// Using `Id::LAZY_INITIALIZER`, while awkward, works fine.
    ///
    /// ```
    /// # use lazy_id::Id;
    /// static ARR: [Id; 2] = [Id::LAZY_INITIALIZER; 2];
    /// assert_ne!(ARR[0], ARR[1]);
    /// ```
    ///
    ///
    /// This API is only present for these sorts of cases, and shouldn't be used
    /// when either [`Id::new`] or [`Id::lazy`] works.
    pub const LAZY_INITIALIZER: Self = Self(AtomicU64::new(0));

    /// Returns the value of this id, lazily initializing if needed.
    ///
    /// Often this function does not need to be called explicitly.
    ///
    /// # Example
    /// ```
    /// # use lazy_id::Id;
    /// let a = Id::lazy();
    /// let b = Id::lazy();
    ///
    /// assert_ne!(a.get(), b.get());
    /// ```
    #[inline]
    pub fn get(&self) -> u64 {
        self.get_nonzero().get()
    }

    /// Initialized id values are never zero, so we can provide this trivially.
    ///
    /// It's unclear how useful it is, although we accept a `NonZeroU64` in
    /// [`Id::from_raw_integer`], and this makes that easier to call.
    ///
    /// # Example
    ///
    /// ```
    /// # use lazy_id::Id;
    /// let a = Id::new();
    /// let manual_clone_of_a = Id::from_raw_integer(a.get_nonzero());
    /// assert_eq!(a, manual_clone_of_a);
    /// ```
    #[inline]
    pub fn get_nonzero(&self) -> NonZeroU64 {
        // Relaxed is fine here because we're only interested in the effect on a
        // single atomic variable.
        if let Some(id) = NonZeroU64::new(self.0.load(Relaxed)) {
            id
        } else {
            let my_id = self.lazy_init();
            debug_assert_eq!(self.0.load(Relaxed), my_id.get());
            my_id
        }
    }

    #[inline]
    fn get_ref(&self) -> &u64 {
        // force initialization
        let _ = self.get();
        // SAFETY: We've definitely been initialized by now, and so our value
        // will never be written to again (or at least, it's no longer has
        // observable interior mutability).
        unsafe { &*(self as *const _ as *const u64) }
    }

    // TODO: Not sure if this should be public, tbh. Might be confusing.
    /// Equivalent to [`Id::get`], but slightly more efficient for first-time
    /// initialization if you have `&mut` access.
    ///
    /// The `&mut` allows us to avoid atomic operations (aside from increment of
    /// the global counter if we're uninitialized), as we know no other threads
    /// are concurrently accessing our data.
    ///
    /// Note that you probably should just use `get()` unless you have a
    /// performance issue or many of these to initialize.
    #[inline]
    fn ensure_init(&mut self) -> NonZeroU64 {
        let ptr: &mut u64 = self.0.get_mut();
        if let Some(nz) = NonZeroU64::new(*ptr) {
            return nz;
        }
        let id = Self::next_id();
        *ptr = id.get();
        id
    }

    // leet ferris
    const ID2SEQ: u64 = 0x1337_fe4415;
    // mult inverse of leet ferris
    const SEQ2ID: u64 = 6848199123282258749;

    #[inline]
    fn next_id() -> NonZeroU64 {
        // static assert that the value is odd, proving safety.
        const _: [(); 1] = [(); (Id::SEQ2ID & 1) as usize];

        let seq = next_seq();
        let id = seq.get().wrapping_mul(Id::SEQ2ID);
        // SAFETY: `SEQ2ID` is odd, e.g. relatively prime with 2^64. this
        // `x.wrapping_add(SEQ2ID)` is reversible — every output is produced by
        // exactly 1 input (in `0..=u64::MAX`). `(0 * SEQ2ID) mod 2^64` is 0, so
        // we know that `0` must be the only u64 such that
        // `x.wrapping_mul(SEQ2ID) == 0` — therefore, it's safe for us to
        // multiply an incoming `NonZeroU64` with SEQ2ID, and put the result in
        // a `NonZeroU64`, as the input not being zero means the output is not
        // as well.
        //
        // See: https://en.wikipedia.org/wiki/Modular_arithmetic and
        // https://en.wikipedia.org/wiki/Modular_multiplicative_inverse for more
        // info
        unsafe {
            // look, just because i have a proof doesn't mean I'm not paranoid.
            debug_assert!(id != 0);
            NonZeroU64::new_unchecked(id)
        }
    }

    #[cold]
    fn lazy_init(&self) -> NonZeroU64 {
        let id = Self::next_id();
        // Relaxed is fine here too because we're only interested in the effect
        // on a single atomic variable. Again, we only care that the ids spit
        // out by `ALLOC` be distinct, and not that they are in any specific
        // order, so the two atomic variables don't need synchronization.
        match self.0.compare_exchange(0, id.get(), Relaxed, Relaxed) {
            Ok(_) => id,
            // Another thread got here first — that's fine, `id` will just
            // go unused.
            Err(e) => {
                debug_assert!(e != 0);
                // Safety: the update failed meaning the current value was not
                // the same.
                unsafe { core::num::NonZeroU64::new_unchecked(e) }
            }
        }
    }

    /// Create an id with a specific internal value. Something of an escape
    /// hatch.
    ///
    /// Internally, we reserve 0 as a sentinel that indicates the Id has not
    /// been initialized and assigned a value yet. This is why we accept a
    /// `NonZeroU64` and not a `u64`. That said, `Id::get_nonzero` or
    /// `NonZeroU64::from(id)` avoid this being too annoying when the input
    /// was from an `Id` originally
    ///
    /// # Caveats
    ///
    /// This function should be used with care, as it compromises the uniqueness
    /// of `Id` — The resulting `Id` may be one with a value we use in the
    /// future, or have used in the past.
    ///
    /// # Example
    /// ```
    /// # use lazy_id::Id;
    /// # use core::num::NonZeroU64;
    /// let v = Id::from_raw_integer(NonZeroU64::new(400).unwrap());
    /// assert_eq!(v.get(), 400);
    /// ```
    #[inline]
    pub const fn from_raw_integer(id: NonZeroU64) -> Self {
        Self(AtomicU64::new(id.get()))
    }
}

impl PartialEq for Id {
    #[inline]
    fn eq(&self, o: &Self) -> bool {
        self.get() == o.get()
    }
}

impl PartialOrd for Id {
    #[inline]
    fn partial_cmp(&self, o: &Self) -> Option<core::cmp::Ordering> {
        self.get().partial_cmp(&o.get())
    }
}

impl Eq for Id {}

impl core::cmp::Ord for Id {
    #[inline]
    fn cmp(&self, o: &Self) -> core::cmp::Ordering {
        self.get().cmp(o)
    }
}

impl core::hash::Hash for Id {
    #[inline]
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.get().hash(state)
    }
}

impl PartialEq<u64> for Id {
    #[inline]
    fn eq(&self, o: &u64) -> bool {
        self.get() == *o
    }
}

impl PartialEq<Id> for u64 {
    #[inline]
    fn eq(&self, o: &Id) -> bool {
        *self == o.get()
    }
}

impl Clone for Id {
    #[inline]
    fn clone(&self) -> Self {
        Self(AtomicU64::new(self.get()))
    }
}

impl core::fmt::Debug for Id {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let v = self.get();
        write!(f, "Id({:#x}; seq={})", v, v.wrapping_mul(Self::ID2SEQ))
    }
}

impl core::fmt::Display for Id {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.get().fmt(f)
    }
}

impl core::ops::Deref for Id {
    type Target = u64;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.get_ref()
    }
}

impl core::borrow::Borrow<u64> for Id {
    #[inline]
    fn borrow(&self) -> &u64 {
        self
    }
}

impl AsRef<u64> for Id {
    #[inline]
    fn as_ref(&self) -> &u64 {
        self
    }
}

impl Default for Id {
    #[inline]
    fn default() -> Self {
        Id::new()
    }
}

impl From<Id> for u64 {
    #[inline]
    fn from(mut id: Id) -> Self {
        id.ensure_init().get()
    }
}

impl From<&Id> for u64 {
    #[inline]
    fn from(id: &Id) -> Self {
        id.get()
    }
}

impl From<Id> for NonZeroU64 {
    #[inline]
    fn from(mut id: Id) -> Self {
        id.ensure_init()
    }
}

static ID_ALLOC: AtomicU64 = AtomicU64::new(1);

#[inline]
fn next_seq() -> NonZeroU64 {
    // Relaxed is fine here, because we only care that this be distinct from
    // other ids — ensured by it being an atomic increment with an overflow
    // check. It's fine and expected that IDs might be skipped. Note that this
    // doesn't need to synchronize in any way with the atomic ops in `sync::Id`.
    let seq = ID_ALLOC.fetch_add(1, Relaxed);
    if seq > (i64::MAX as u64) {
        // Protect against overflow (which would take decades) by aborting
        // (bringing down just our thread by panicing isn't sufficient).
        // Testing the `seq > i64::MAX` (and not `seq == 0`) avoids the case
        // where a thread allocate the seq that causes the wrap, and is
        // suspended before the check. During the period when it's
        // suspended, some number of ids may be allocated, which would
        // be duplicates of existing ids.
        nostd_abort();
    }
    debug_assert!(seq != 0);
    // Safety: we start at 1, and protect against overflow, so `seq` can't be 0.
    unsafe { NonZeroU64::new_unchecked(seq) }
}

#[cfg(test)]
mod test {
    #[test]
    fn mixing() {
        // no longer have `unsync`...

        fn syncmix(u: u64) -> u64 {
            u.wrapping_mul(super::Id::SEQ2ID)
        }
        fn syncunmix(u: u64) -> u64 {
            u.wrapping_mul(super::Id::ID2SEQ)
        }
        let count = if cfg!(miri) { 100 } else { 10000 };
        // true for all integers, holds because they're odd and becuase of
        // the properties of
        // https://en.wikipedia.org/wiki/Modular_multiplicative_inverse
        for i in 0..count {
            let v = [i, !i, syncmix(i), syncunmix(i)];
            for (j, v) in v.iter().copied().enumerate() {
                assert_eq!(syncunmix(syncmix(v)), v, "i: {} step {}", i, j);
                assert_eq!(syncmix(syncunmix(v)), v, "i: {} step {}", i, j);
            }
        }
    }
}

#[cold]
#[inline(never)]
fn nostd_abort() -> ! {
    struct PanicOnDrop();
    impl Drop for PanicOnDrop {
        #[inline]
        fn drop(&mut self) {
            panic!("Id counter overflow. Aborting by double panic (2/2)");
        }
    }
    let _p = PanicOnDrop();
    panic!("Id counter overflow. Aborting by double panic (1/2)");
}
