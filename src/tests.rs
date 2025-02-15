#![cfg(test)]

use crate::Data;

#[test]
fn test_by_val_and_by_ref() {
    let mut orig = Data::value(0);
    assert!(orig.is_val());
    let mut valu = orig.by_val();
    assert!(valu.is_val());
    let mut refr = orig.by_ref();
    assert!(orig.is_ref());
    assert!(refr.is_ref());

    assert_eq!(0, *orig.borrow());

    *valu.borrow_mut() = 1;
    *refr.borrow_mut() = 2;

    assert_eq!(2, *orig.borrow());
    assert_eq!(1, *valu.borrow());
    assert_eq!(2, *refr.borrow());
}

#[test]
fn test_by_cow_mutability() {
    let mut data = Data::value("hello");
    assert!(data.is_val());
    assert_eq!(*data.borrow(), "hello");
    let mut cow = data.by_cow();
    assert_eq!(*cow.borrow(), "hello");
    assert_eq!(*data.borrow(), "hello");
    assert!(data.is_cow());
    assert!(cow.is_cow());

    assert_eq!(*cow.borrow(), "hello");
    *cow.borrow_mut() = "goodbye";
    assert_eq!(*data.borrow(), "hello");
    assert_eq!(*cow.borrow(), "goodbye");
}

#[test]
fn test_ref_conversion() {
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

#[test]
fn test_cow_conversion() {
    let mut data = Data::value("Test".to_string());
    assert!(data.is_val());

    let mut cow_data = data.by_cow();
    assert!(cow_data.is_cow());
    assert!(data.is_cow());

    assert_eq!(*cow_data.borrow(), "Test");

    *cow_data.borrow_mut() = "Modified".to_string();
    assert_eq!(*cow_data.borrow(), "Modified");
    assert_eq!(*data.borrow(), "Test"); // Original remains unchanged
}

#[test]
fn test_cow_to_ref() {
    let mut data = Data::value(10);
    assert!(data.is_val());

    let mut cow_data = data.by_cow();
    assert!(cow_data.is_cow());
    let mut ref_data = cow_data.by_ref();

    assert!(ref_data.is_ref());
    assert_eq!(*ref_data.borrow(), 10);

    *ref_data.borrow_mut() += 5;
    assert_eq!(*ref_data.borrow(), 15);
}

#[test]
fn test_ref_shared_mutation() {
    let mut ref_data = Data::reference(5);
    assert!(ref_data.is_ref());

    let mut ref_clone = ref_data.by_ref();
    assert!(ref_clone.is_ref());

    *ref_clone.borrow_mut() *= 2;
    assert_eq!(*ref_data.borrow(), 10);
    assert_eq!(*ref_clone.borrow(), 10);
}

#[test]
fn test_multiple_cow_clones() {
    let mut data = Data::cow(99);
    assert!(data.is_cow());

    let mut cow1 = data.by_cow();
    let cow2 = data.by_cow();

    assert!(cow1.is_cow());
    assert!(cow2.is_cow());

    *cow1.borrow_mut() = 88;
    assert_eq!(*cow1.borrow(), 88);
    assert_eq!(*data.borrow(), 99); // Data remains unchanged
}

#[test]
fn test_borrow_and_mut_borrow() {
    let mut data = Data::value(100);
    let borrowed = data.borrow();
    assert_eq!(*borrowed, 100);

    drop(borrowed); // Ensure borrow ends before mutable borrow

    let mut borrowed_mut = data.borrow_mut();
    *borrowed_mut += 50;
    assert_eq!(*borrowed_mut, 150);
}

#[test]
fn test_cow_does_not_affect_original() {
    let mut data = Data::value("unchanged".to_string());
    let mut cow_data = data.by_cow();

    assert_eq!(*data.borrow(), "unchanged");
    assert_eq!(*cow_data.borrow(), "unchanged");

    *cow_data.borrow_mut() = "changed".to_string();

    assert_eq!(*cow_data.borrow(), "changed");
    assert_eq!(*data.borrow(), "unchanged"); // Original remains unaffected
}

#[test]
fn test_deeply_nested_cow() {
    let mut data = Data::cow(42);
    let mut cow1 = data.by_cow();
    let mut cow2 = cow1.by_cow();

    assert!(cow1.is_cow());
    assert!(cow2.is_cow());

    *cow2.borrow_mut() = 99;

    assert_eq!(*cow1.borrow(), 42); // Original is unchanged
    assert_eq!(*cow2.borrow(), 99); // Modified copy
}
