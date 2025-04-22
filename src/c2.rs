use std::cell::{Ref, RefCell, RefMut, UnsafeCell};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use crate::take::take;

#[derive(Debug)]
enum Defer<T: Clone> {
    // guarantee terminal
    Own(T),

    // guarantee Data::Ref
    Ptr(Data<T>)
}

pub enum Data<T: Clone> {
    Value(T),
    Ref(Rc<RefCell<Defer<T>>>),
}

impl<T: Clone> Data<T> {
    pub fn by_val(&self) -> Self {
        match self {
            Data::Value(value) => {
                let copy = T::clone(value);
                Data::Value(copy)
            }
            Data::Ref(r) => match r.borrow().deref() {
                Defer::Own(own) => {
                    let copy = T::clone(own);
                    Data::Value(copy)
                }
                Defer::Ptr(data) => {
                    data.by_val()
                }
            }
        }
    }

    pub fn by_ref(&mut self) -> Self {
        take(self, |data| {
            let (data, copied) = match data {
                Data::Value(v) => {
                    let one = Rc::new(RefCell::new(Defer::Own(v)));
                    let two = Rc::clone(&one);

                    (Data::Ref(one), Data::Ref(two))
                }
                Data::Ref(r) => {
                    let mut defer = r.borrow_mut();
                    match defer.deref_mut() {
                        Defer::Own(_own) => {
                            let copied = Rc::clone(&r);
                            drop(defer); // drop the borrow
                            (Data::Ref(r), Data::Ref(copied))
                        }
                        Defer::Ptr(ptr) => {
                            // this will flatten the Defer
                            let mut data = ptr.by_ref();
                            let copied = data.by_ref();
                            (data, copied)
                        }
                    }
                }
            };

            (data, copied)
        })
    }

    /// Replace the value stored in `self` with the terminal value found in
    /// `other`, propagating the change to all existing aliases of `self`.
    pub fn set(&mut self, mut other: Data<T>) {
        // pull out a fresh clone of the *actual* value inside `other`
        let new_val: T = (*other.borrow()).clone();

        // take moves out self, lets us mutate it, and then puts it back
        take(self, move |mut this| {
            match &mut this {
                // No other aliases exist – just overwrite the scalar
                Data::Value(cur) => {
                    *cur = new_val;
                }

                // we already have a Rc; update the pointed to value, so every
                // alias of the same Rc observes the change
                Data::Ref(rc) => {
                    // make sure `rc` is flattened first.
                    Self::un_defer(rc);

                    let mut defer = rc.borrow_mut();
                    match &mut *defer {
                        Defer::Own(cur) => *cur = new_val,
                        Defer::Ptr(_)    => unreachable!("compression failed"),
                    }
                }
            }

            // put the (possibly modified) Data back into self
            (this, ())
        });
    }
}

impl<T: Clone> Data<T> {
    

    
    
    /// dont use this whenever possible. this is a last resort. it bypasses 
    pub fn borrow_no_compress(&self) -> ValRef<'_, T> {
        match self {
            // trivial case – plain value on the stack
            Data::Value(v) => ValRef::Raw(v),

            // shared, possibly‑deferred value behind an Rc<RefCell<…>>
            Data::Ref(rc) => {
                // optimistic read
                let defer = rc.borrow();

                // fast‑path: already `Own`
                if let Defer::Own(_) = &*defer {
                    return ValRef::Ref(Ref::map(defer, |d| match d {
                        Defer::Own(v) => v,
                        Defer::Ptr(_)  => unreachable!(),
                    }));
                }

                // slow‑path: we’ve hit a pointer – resolve & compress
                let target = match &*defer {
                    Defer::Ptr(next) => next,
                    _                => unreachable!(),
                };

                // clone the *terminal* value while we still hold only a
                // shared borrow on `rc` (safe because `target` is a different
                // RefCell)
                let value_clone = {
                    let v_ref = target.borrow_no_compress();
                    (*v_ref).clone()
                };

                // release the shared borrow so we can mutate
                drop(defer);

                // overwrite `Ptr` → `Own(cloned_value)`  (path compression)
                {
                    let mut defer_mut = rc.borrow_mut();
                    *defer_mut = Defer::Own(value_clone);
                }

                // 3️⃣  Now we’re definitely `Own`; create a mapped Ref
                let defer = rc.borrow();
                ValRef::Ref(Ref::map(defer, |d| match d {
                    Defer::Own(v) => v,
                    Defer::Ptr(_) => unreachable!("compression failed"),
                }))
            }
        }
    }

    pub fn borrow(&mut self) -> ValRef<'_, T> {
        match self {
            Data::Value(v) => ValRef::Raw(v),
            Data::Ref(r) => {
                Self::un_defer(r);
                ValRef::Ref(Ref::map(r.borrow(), |defer| match defer {
                    Defer::Own(v) => v,
                    Defer::Ptr(_) => unreachable!("compression failed"),
                }))
            }
        }
    }

    pub fn borrow_mut(&mut self) -> ValRefMut<'_, T> {
        match self {
            Data::Value(v) => ValRefMut::Raw(v),
            Data::Ref(r) => {
                Self::un_defer(r);
                ValRefMut::Ref(RefMut::map(r.borrow_mut(), |defer| match defer {
                    Defer::Own(v) => v,
                    Defer::Ptr(_) => unreachable!("compression failed"),
                }))
            }
        }
    }
}

impl<T: Clone> Data<T> {
    #[inline]
    pub fn value(data: T) -> Self {
        Self::Value(data)
    }

    #[inline]
    pub fn refer(data: T) -> Self {
        Self::Ref(Rc::new(RefCell::new(Defer::Own(data))))
    }
}

impl<T: Clone> Data<T> {
    pub fn is_ref(&self) -> bool {
        matches!(self, Data::Ref(_))
    }

    pub fn is_val(&self) -> bool {
        matches!(self, Data::Value(_))
    }
}

impl<T> Clone for Data<T> where T: Clone {
    /// An alias for [Self::by_val]
    fn clone(&self) -> Self {
        self.by_val()
    }
}

pub enum ValRef<'a, T: Clone + 'a> {
    Raw(&'a T),
    Ref(Ref<'a, T>),
    Rec(&'a ValRef<'a, T>)
}

impl<T: ?Sized + Clone> Deref for ValRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Rec(rec) => rec.deref(), // recursive deref
            Self::Raw(v) => *v,
            Self::Ref(r) => &*r,
        }
    }
}

pub enum ValRefMut<'a, T: Clone + 'a> {
    Raw(&'a mut T),
    Ref(RefMut<'a, T>),
}

impl<T: ?Sized + Clone> Deref for ValRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(r) => &*r,
        }
    }
}

impl<T: ?Sized + Clone> DerefMut for ValRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(r) => &mut *r,
        }
    }
}

impl<T: Clone> Data<T> {
    /// Recursively follow any `Defer::Ptr` chain and clone the terminal value.
    fn clone_terminal_rc(data: &Data<T>) -> Rc<RefCell<Defer<T>>> {
        match data {
            Data::Value(v) => Rc::new(RefCell::new(Defer::Own(v.clone()))),
            Data::Ref(r) => {
                let defer = r.borrow();
                match &*defer {
                    Defer::Own(_v) => Rc::clone(&r),
                    Defer::Ptr(next) => Self::clone_terminal_rc(next),
                }
            }
        }
    }

    /// Flatten a single `Rc<RefCell<Defer<T>>>` so that it becomes `Defer::Own`.
    fn un_defer(rc: &mut Rc<RefCell<Defer<T>>>) {
        // fast path: already compressed.
        if matches!(*rc.borrow(), Defer::Own(_)) {
            return;
        }

        let root = {
            let defer = rc.borrow();
            let Defer::Ptr(target) = &*defer else {
                // SAFETY: covered by the early‑out above.
                return;
            };
            Self::clone_terminal_rc(target)
        };

        // replace the pointer with the new pointer
        *rc = root;
    }

    /// Compress the current `Data` in place.
    pub fn compress(&mut self) {
        if let Data::Ref(r) = self {
            Self::un_defer(r);
        }
    }
}

impl<T> Debug for Data<T> where T: Debug + Clone {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Data::Value(v) => {
                f.debug_tuple("Val")
                    .field(v)
                    .finish()
            }
            Data::Ref(r) => {
                let r = &*r.borrow();
                f.debug_tuple("Ref")
                    .field(r)
                    .finish()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use super::Data;

    /// Verify that `by_val` clones the value while `by_ref` shares it.
    #[test]
    fn test_by_val_and_by_ref() {
        let mut orig = Data::value(0);
        assert!(orig.is_val());

        let mut valu = orig.by_val(); // deep copy
        assert!(valu.is_val());

        let mut refr = orig.by_ref(); // shared pointer
        assert!(orig.is_ref());
        assert!(refr.is_ref());

        assert_eq!(*orig.borrow(), 0);

        *valu.borrow_mut() = 1; // only the copy changes
        *refr.borrow_mut() = 2; // shared copy updates both

        assert_eq!(*orig.borrow(), 2);
        assert_eq!(*valu.borrow(), 1);
        assert_eq!(*refr.borrow(), 2);
    }

    /// Converting a `Value` to a `Ref`, then mutating through either handle,
    /// always reflects the change everywhere.
    #[test]
    fn test_ref_conversion_and_mutation() {
        let mut data = Data::value(100);
        assert!(data.is_val());

        let mut ref_data = data.by_ref();
        assert!(ref_data.is_ref());
        assert!(data.is_ref());

        assert_eq!(*ref_data.borrow(), 100);

        *ref_data.borrow_mut() += 50;
        assert_eq!(*ref_data.borrow(), 150);
        assert_eq!(*data.borrow(), 150);
    }

    /// Two `by_ref` calls on the same node share the underlying `Rc`.
    #[test]
    fn test_ref_shared_mutation() {
        let mut ref_data = Data::refer(5);
        assert!(ref_data.is_ref());

        let mut ref_clone = ref_data.by_ref();
        assert!(ref_clone.is_ref());

        *ref_clone.borrow_mut() *= 2;
        assert_eq!(*ref_data.borrow(), 10);
        assert_eq!(*ref_clone.borrow(), 10);
    }

    /// A `borrow()` must end before a `borrow_mut()`.
    #[test]
    fn test_borrow_then_mut_borrow() {
        let mut data = Data::value(100);
        {
            let borrowed = data.borrow();
            assert_eq!(*borrowed, 100);
            // `borrowed` dropped here
        }

        {
            let mut borrowed_mut = data.borrow_mut();
            *borrowed_mut += 50;
            assert_eq!(*borrowed_mut, 150);
        }
    }

    /// Path compression: after first access, intermediate `Defer::Ptr`
    /// becomes an `Own`, so later borrows borrow just one cell.
    #[test]
    fn test_path_compression() {
        // Build a chain: Ptr -> Ptr -> Value(7)
        let leaf = Data::value(7);
        let middle = Data::Ref(Rc::new(RefCell::new(super::Defer::Ptr(leaf))));
        let mut root = Data::Ref(Rc::new(RefCell::new(super::Defer::Ptr(middle))));

        println!("{:#?}", root);
        // First borrow flattens the chain
        assert_eq!(*root.borrow(), 7);

        println!("{:#?}", root);
        // A second borrow should not panic and still return 7
        assert_eq!(*root.borrow(), 7);
    }

    #[test]
    fn test_set() {
        let mut sub = Data::value(7);
        let mut dangle = sub.by_ref();

        let mut root = Data::refer(10);
        let mut leaf1 = root.by_ref();
        let mut leaf2 = root.by_ref();

        
        // println!("{:#?}", root); // should be 7
        assert_eq!(*sub.borrow_mut(), 7);
        assert_eq!(*dangle.borrow_mut(), 7);
        
        assert_eq!(*root.borrow_mut(), 10);
        assert_eq!(*leaf1.borrow_mut(), 10);
        assert_eq!(*leaf2.borrow_mut(), 10);
        
        root.set(sub);


        assert_eq!(*dangle.borrow(), 7);

        assert_eq!(*root.borrow(), 7);
        assert_eq!(*leaf1.borrow(), 7);
        assert_eq!(*leaf2.borrow(), 7);
    }

    /// A practical nested‑collection example that mixes `Value` and `Ref`.
    #[test]
    fn test_nested_collection_edit() {
        // Data<Vec<Data<String>>>
        let build_collection = || {
            let mut v = Data::refer(Vec::new());
            for i in 1..=5 {
                v.borrow_mut().push(Data::value(i.to_string()));
            }
            v
        };

        let mut col = build_collection();
        let mut col_ref = col.by_ref(); // shared pointer to the vec
        {
            // clone element[0] by value, keep a copy for later
            let mut first_copy = col_ref.borrow_mut()[0].by_val();
            assert_eq!(*first_copy.borrow(), "1");

            // mutate element[0] through the shared `Ref`
            *col_ref.borrow_mut()[0].borrow_mut() = "X".to_string();

            // The original collection changed …
            let snapshot: Vec<_> = col
                .borrow()
                .iter()
                .map(|d| d.borrow_no_compress().clone())
                .collect();
            
            assert_eq!(snapshot, ["X", "2", "3", "4", "5"]);

            // … but the by‑value copy did not.
            assert_eq!(*first_copy.borrow(), "1");
        }
    }
}


#[cfg(test)]
mod additional_tests {
    use super::Data;

    /// 1. Cloning a `Ref` yields a `Value` (deep clone)
    #[test]
    fn test_clone_returns_value() {
        let orig = Data::refer(42);
        assert!(orig.is_ref());
        let mut cloned = orig.clone();
        assert!(cloned.is_val());
        assert_eq!(*cloned.borrow(), 42);
    }

    /// 2. set on two Values updates correctly
    #[test]
    fn test_set_value_to_value() {
        let mut d = Data::value(1);
        d.set(Data::value(2));
        assert_eq!(*d.borrow(), 2);
    }

    /// 3. set a Ref with a Value
    #[test]
    fn test_set_ref_to_value() {
        let mut root = Data::refer(1);
        let mut handle = root.by_ref();
        root.set(Data::value(3));
        assert_eq!(*handle.borrow(), 3);
    }

    /// 4. set a Value with a Ref
    #[test]
    fn test_set_value_to_ref() {
        let mut root = Data::value(1);
        let mut handle = root.by_ref();
        root.set(Data::refer(5));
        assert_eq!(*handle.borrow(), 5);
    }

    /// 5. Repeated set calls propagate
    #[test]
    fn test_repeated_set() {
        let mut root = Data::refer(0);
        let mut handle = root.by_ref();
        root.set(Data::value(10));
        root.set(Data::value(20));
        assert_eq!(*handle.borrow(), 20);
    }

    /// 6. compress on a Value is no-op
    #[test]
    fn test_compress_on_value() {
        let mut v = Data::value(5);
        v.compress();
        assert!(v.is_val());
        assert_eq!(*v.borrow(), 5);
    }

    /// 7. compress on a Ref flattens repeatedly
    #[test]
    fn test_compress_on_ref() {
        let leaf = Data::value(7);
        let mid = Data::refer(leaf.clone().borrow().clone());
        let mut root = Data::refer(mid.clone().borrow().clone());
        // multiple compress calls
        root.compress();
        root.compress();
        assert_eq!(*root.borrow(), 7);
    }

    /// 8. Nested Vec by_val produces deep clone
    #[test]
    fn test_nested_by_val_deep_clone() {
        let mut col = Data::refer(vec![Data::value(1), Data::value(2)]);
        let mut copy = col.by_val();
        // mutate original
        *col.borrow_mut()[0].borrow_mut() = 99;
        // copy unchanged
        assert_eq!(*copy.borrow_mut()[0].borrow(), 1);
    }

    /// 9. Nested Vec by_ref shares underlying
    #[test]
    fn test_nested_by_ref_shared() {
        let mut col = Data::refer(vec![Data::value(1), Data::value(2)]);
        let mut r1 = col.by_ref();
        *r1.borrow_mut()[1].borrow_mut() = 88;
        assert_eq!(*col.borrow_mut()[1].borrow(), 88);
    }

    /// 10. Mixed by_ref then set
    #[test]
    fn test_mixed_by_ref_value_then_set() {
        let mut root = Data::value(100);
        let mut handle = root.by_ref();
        root.set(Data::value(50));
        assert_eq!(*handle.borrow(), 50);
    }

    /// 11. borrow after drop of previous borrow
    #[test]
    fn test_multiple_borrows() {
        let mut d = Data::value(20);
        {
            let b = d.borrow();
            assert_eq!(*b, 20);
        }
        {
            let mut bm = d.borrow_mut();
            *bm += 5;
            assert_eq!(*bm, 25);
        }
        assert_eq!(*d.borrow(), 25);
    }

    /// 13. set with Ref does not share other Rc
    #[test]
    fn test_set_ref_does_not_share_rc() {
        let mut root = Data::refer(1);
        let other = Data::refer(2);
        let mut h_root = root.by_ref();
        root.set(other.clone());
        drop(other);
        assert_eq!(*h_root.borrow(), 2);
    }

    /// 14. multiple mutations through borrow_mut
    #[test]
    fn test_val_ref_mut_multiple_mutations() {
        let mut root = Data::refer(5);
        let mut r1 = root.by_ref();
        *r1.borrow_mut() += 1;
        *r1.borrow_mut() += 2;
        assert_eq!(*root.borrow(), 8);
    }

    /// 15. clone() on complex Data
    #[test]
    fn test_clone_after_nested_by_ref() {
        let mut root = Data::refer(vec![Data::value(10)]);
        let mut r = root.by_ref();
        *r.borrow_mut()[0].borrow_mut() = 42;
        let mut deep_clone = root.clone();
        assert!(deep_clone.is_val());
        assert_eq!(*deep_clone.borrow_mut()[0].borrow(), 42);
    }
}