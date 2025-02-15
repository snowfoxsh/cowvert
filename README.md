A simple library for creating references and copy on write values.

It is designed to be used in interpreters for handling the env.

See the tests to learn more. 

---

### Example Usage

#### Converting Between Value and Reference

```rust
use cowvert::Data;

fn main() {
    let mut data = Data::value(100);
    assert!(data.is_val());

    let mut ref_data = data.by_ref();
    assert!(ref_data.is_ref());

    *ref_data.borrow_mut() += 50;
    
    assert_eq!(*data.borrow(), 150); // mutates the original
}
```

#### Copy-on-Write (COW)

```rust
use cowvert::Data;

fn main() {
    let mut data = Data::value("hello".to_string());

    let mut cow_data = data.by_cow();
    *cow_data.borrow_mut() = "goodbye".to_string();

    assert_eq!(*data.borrow(), "hello"); // original remains unchanged
    assert_eq!(*cow_data.borrow(), "goodbye"); // copy is modified
}
```
