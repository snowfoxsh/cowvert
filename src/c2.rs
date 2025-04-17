use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::fmt;
use std::fmt::{Debug, Formatter};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;
use crate::take::take;

#[derive(Debug)]
enum Defer<T: Clone> {
    // guarantee bottom of stack
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
}

impl<T: Clone> Data<T> {
    pub fn borrow(&self) -> ValRef<'_, T> {
        match self {
            Data::Value(v) => ValRef::Raw(v),
            Data::Ref(r) => ValRef::Ref(r.as_ref().borrow()),
        }
    }

    pub fn borrow_mut(&mut self) -> ValRefMut<'_, T> {
        match self {
            Data::Value(v) => ValRefMut::Raw(v),
            Data::Ref(r) => ValRefMut::Ref(r.borrow_mut()),
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


impl<T> Clone for Data<T> where T: Clone {
    fn clone(&self) -> Self {
        self.by_val()
    }
}

pub enum ValRef<'a, T: ?Sized + Clone + 'a> {
    Raw(&'a T),
    Ref(Ref<'a, Defer<T>>),
}

impl<T: ?Sized + Clone> Deref for ValRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => *v,
            Self::Ref(r) => {
                match r.deref() {
                    Defer::Own(_) => {}
                    Defer::Ptr(_) => {}
                }
            }
        }
    }
}

pub enum ValRefMut<'a, T: ?Sized + Clone + 'a> {
    Raw(&'a mut T),
    Ref(RefMut<'a, Defer<T>>),
}

impl<T: ?Sized + Clone> Deref for ValRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(v) => {
                // let defer = 
            },
        }
    }
}

impl<T: ?Sized + Clone> DerefMut for ValRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Raw(v) => v,
            Self::Ref(v) => {
                
            }
        }
    }
}

fn test() {
    let cell = RefCell::new("hello world");

    {
        let mut o = cell.borrow_mut();
        let mut t = cell.borrow_mut();
        let im = cell.borrow();

        {
            o.deref_mut();
        }
        t.deref_mut();
    }
}