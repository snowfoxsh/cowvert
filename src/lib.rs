mod tests;

use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

#[derive(Debug)]
enum Data<T: Clone> {
    Value(Option<T>),
    Ref(Rc<RefCell<T>>),
    Cow(Option<Rc<Option<T>>>),
}

impl<T: Clone> Data<T> {
    pub fn by_val(&self) -> Self {
        match self {
            Data::Value(v) => {
                // clone <T>
                Data::Value(v.clone())
            }
            Data::Ref(r) => {
                // clone <T>
                let t = r.as_ref().borrow().clone();
                Data::Value(Some(t))
            }
            Data::Cow(c) => {
                // will always succeed
                let c = c.as_ref().unwrap();
                // clone <T>
                let t = c.as_ref().clone();
                Data::Value(t)
            }
        }
    }

    pub fn by_ref(&mut self) -> Self {
        match self {
            Data::Ref(r) => {
                // just clone the ptr to the cell
                Data::Ref(Rc::clone(r))
            }
            Data::Value(opt) => {
                // take T out of the Option
                let t = opt.take().expect("Value was already taken");
                let rc = Rc::new(RefCell::new(t));
                // replace self with the Ref variant
                *self = Data::Ref(Rc::clone(&rc));
                Data::Ref(rc)
            }
            Data::Cow(c) => {
                // take out the Rc<Option<T>> ensuring it exists
                let mut c = c.take().expect("Value was already taken");

                let rc = if let Some(inner) = Rc::get_mut(&mut c) {
                    // own T
                    let t = inner.take().expect("Inner option was empty");
                    Rc::new(RefCell::new(t))
                } else {
                    // clone <T>
                    let t = c.as_ref().clone().expect("Inner option was empty");
                    Rc::new(RefCell::new(t))
                };

                // replace `self` with the Ref variant
                *self = Data::Ref(Rc::clone(&rc));
                Data::Ref(rc)
            }
        }
    }

    pub fn by_cow(&mut self) -> Self {
        match self {
            Data::Value(v) => {
                // create share <T>
                let rc = Rc::new(v.take());

                // upgrade self into a Cow
                *self = Data::Cow(Some(Rc::clone(&rc)));
                Data::Cow(Some(rc))
            }
            Data::Ref(r) => {
                // copy <T>
                let copy = r.borrow().clone();
                // return the new Cow
                Data::Cow(Some(Rc::new(Some(copy))))
            }
            Data::Cow(c) => {
                // own Rc<T>
                let rc = c.take().expect("Value was already taken");
                // replace the taken Rc<T>
                *self = Data::Cow(Some(Rc::clone(&rc)));
                // return the original Rc<T>
                Data::Cow(Some(rc))
            }
        }
    }
}

impl<T: Clone> Data<T> {
    pub fn borrow(&self) -> ValRef<'_, T> {
        match self {
            Data::Value(Some(v)) => ValRef::Raw(v),
            Data::Ref(r) => ValRef::Ref(r.as_ref().borrow()),
            Data::Cow(Some(v)) => {
                let t = v.as_ref().as_ref().unwrap();
                ValRef::Raw(t)
            }
            _ => unreachable!("Value was already taken"),
        }
    }

    pub fn borrow_mut(&mut self) -> ValRefMut<'_, T> {
        match self {
            Data::Value(Some(v)) => ValRefMut::Raw(v),
            Data::Ref(r) => ValRefMut::Ref(r.borrow_mut()),
            Data::Cow(Some(v)) => {
                if let Some(own) = Rc::get_mut(v) {
                    // take ownership of <T>
                    let t = own.take();
                    *self = Data::Value(t);
                    self.borrow_mut()
                } else {
                    // make copy of <T>
                    let tc = v.as_ref().clone();
                    *self = Data::Value(tc);
                    self.borrow_mut()
                }
            }
            _ => unreachable!("Value was already taken"),
        }
    }
}

impl<T: Clone> Data<T> {
    #[inline]
    pub fn value(data: T) -> Self {
        Self::Value(Some(data))
    }

    #[inline]
    pub fn reference(data: T) -> Self {
        Self::Ref(Rc::new(RefCell::new(data)))
    }

    #[inline]
    pub fn cow(data: T) -> Self {
        Self::Cow(Some(Rc::new(Some(data))))
    }
}

impl<T: Clone> Data<T> {
    pub fn is_ref(&self) -> bool {
        matches!(self, Data::Ref(_))
    }

    pub fn is_val(&self) -> bool {
        matches!(self, Data::Value(_))
    }

    pub fn is_cow(&self) -> bool {
        matches!(self, Data::Cow(_))
    }
}

enum ValRef<'a, T: ?Sized + 'a> {
    Raw(&'a T),
    Ref(Ref<'a, T>),
}

impl<T: ?Sized> Deref for ValRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => *v,
            Self::Ref(v) => v.deref(),
        }
    }
}

enum ValRefMut<'a, T: ?Sized + 'a> {
    Raw(&'a mut T),
    Ref(RefMut<'a, T>),
}

impl<T: ?Sized> Deref for ValRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(v) => v.deref(),
        }
    }
}

impl<T: ?Sized> DerefMut for ValRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(v) => v.deref_mut(),
        }
    }
}
