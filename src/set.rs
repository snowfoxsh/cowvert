use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use crate::take::take;

#[derive(Debug)]
pub(crate) enum Defer<T: Clone> {
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
    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&T) -> R {
        match self {
            Data::Value(t) => f(t),
            Data::Ref(r) => {
                match r.borrow_mut().deref_mut() {
                    Defer::Own(t) => f(t),
                    Defer::Ptr(data) => {
                        // borrow will preform limited path compression
                        f(data.borrow().deref())
                    }
                }
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
}

impl<T: Clone> Deref for ValRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(r) => r,
        }
    }
}

pub enum ValRefMut<'a, T: Clone + 'a> {
    Raw(&'a mut T),
    Ref(RefMut<'a, T>),
}

impl<T: Clone> Deref for ValRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(r) => r,
        }
    }
}

impl<T: Clone> DerefMut for ValRefMut<'_, T> {
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
