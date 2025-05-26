#![cfg(test)]

use std::cell::RefCell;
use std::rc::Rc;
use crate::Data;
use crate::set::Defer;

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

    let stat = refr.by_ref();
    
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
    let middle = Data::Ref(Rc::new(RefCell::new(Defer::Ptr(leaf))));
    let mut root = Data::Ref(Rc::new(RefCell::new(Defer::Ptr(middle))));

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
            .borrow_mut()
            .iter_mut()
            .map(|d| d.borrow().clone())
            .collect();

        assert_eq!(snapshot, ["X", "2", "3", "4", "5"]);

        // … but the by‑value copy did not.
        assert_eq!(*first_copy.borrow(), "1");
    }
}

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