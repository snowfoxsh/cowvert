use std::cell::{Ref, RefCell, RefMut};
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

    pub fn set(&mut self, other: Data<T>) {
        take(self, move |mut this| {
            // pull out the terminal T from whatever other is
            let new_val = Self::clone_terminal(&other);

            // write it into this in-place
            match &mut this {
                Data::Value(cur) => {
                    *cur = new_val;
                }
                Data::Ref(rc) => {
                    // flatten our own Rc to an Own(T)
                    Self::un_defer(rc);
                    // now overwrite that T
                    if let Defer::Own(cur) = &mut *rc.borrow_mut() {
                        *cur = new_val;
                    }
                }
            }

            // return the same this (so we never swap out the Rc)
            (this, ())
        });
    }
}

impl<T: Clone> Data<T> {
    pub fn borrow(&self) -> ValRef<'_, T> {
        match self {
            Data::Value(v) => ValRef::Raw(v),
            Data::Ref(r) => {
                Self::un_defer(r);
                ValRef::Ref(Ref::map(r.borrow(), |defer| match defer {
                    Defer::Own(v) => v,
                    Defer::Ptr(data) => unreachable!("compression failed"),
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
    fn clone(&self) -> Self {
        self.by_val()
    }
}

pub enum ValRef<'a, T: ?Sized + Clone + 'a> {
    Raw(&'a T),
    Ref(Ref<'a, T>),
}

impl<T: ?Sized + Clone> Deref for ValRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => *v,
            Self::Ref(r) => &*r,
        }
    }
}

pub enum ValRefMut<'a, T: ?Sized + Clone + 'a> {
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
    fn clone_terminal(data: &Data<T>) -> T {
        match data {
            Data::Value(v) => v.clone(),
            Data::Ref(r) => {
                let defer = r.borrow();
                match &*defer {
                    Defer::Own(v) => v.clone(),
                    Defer::Ptr(next) => Self::clone_terminal(next),
                }
            }
        }
    }

    /// Flatten a single `Rc<RefCell<Defer<T>>>` so that it becomes `Defer::Own`.
    fn un_defer(rc: &Rc<RefCell<Defer<T>>>) {
        // Fast path: already compressed.
        if matches!(*rc.borrow(), Defer::Own(_)) {
            return;
        }

        // clone the value at the end of the chain without
        // holding a mutable borrow on `rc`
        let value = {
            let defer = rc.borrow();
            let Defer::Ptr(target) = &*defer else {
                // SAFETY: covered by the early‑out above.
                return;
            };
            Self::clone_terminal(target)
        };

        // now replace the pointer with the owned value
        *rc.borrow_mut() = Defer::Own(value);
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
        assert_eq!(*sub.borrow(), 7);
        assert_eq!(*dangle.borrow(), 7);
        
        assert_eq!(*root.borrow(), 10);
        assert_eq!(*leaf1.borrow(), 10);
        assert_eq!(*leaf2.borrow(), 10);
        
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
            let first_copy = col_ref.borrow()[0].by_val();
            assert_eq!(*first_copy.borrow(), "1");

            // mutate element[0] through the shared `Ref`
            *col_ref.borrow_mut()[0].borrow_mut() = "X".to_string();

            // The original collection changed …
            let snapshot: Vec<_> = col
                .borrow()
                .iter()
                .map(|d| d.borrow().clone())
                .collect();
            assert_eq!(snapshot, ["X", "2", "3", "4", "5"]);

            // … but the by‑value copy did not.
            assert_eq!(*first_copy.borrow(), "1");
        }
    }
}
