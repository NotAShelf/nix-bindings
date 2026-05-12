use std::ptr::NonNull;

use crate::{Error, Result, Value, ValueType, sys};

impl Value<'_> {
  /// Check if this value is a list.
  ///
  /// # Example
  ///
  /// ```rust,no_run
  /// use std::sync::Arc;
  ///
  /// use nix_bindings::{Context, EvalStateBuilder, Store};
  /// fn main() -> Result<(), Box<dyn std::error::Error>> {
  ///   let ctx = Arc::new(Context::new()?);
  ///   let store = Arc::new(Store::open(&ctx, None)?);
  ///   let state = EvalStateBuilder::new(&store)?.build()?;
  ///   let list = state.eval_from_string("[1 2 3]", "<eval>")?;
  ///   assert!(list.is_list());
  ///   Ok(())
  /// }
  /// ```
  #[must_use]
  pub fn is_list(&self) -> bool {
    self.value_type() == ValueType::List
  }

  /// Get the length of this list.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a list.
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use std::sync::Arc;
  /// # use nix_bindings::{Context, EvalStateBuilder, Store};
  /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
  /// # let ctx = Arc::new(Context::new()?);
  /// # let store = Arc::new(Store::open(&ctx, None)?);
  /// # let state = EvalStateBuilder::new(&store)?.build()?;
  /// let list = state.eval_from_string("[1 2 3]", "<eval>")?;
  /// assert_eq!(list.list_len()?, 3);
  /// # Ok(())
  /// # }
  /// ```
  pub fn list_len(&self) -> Result<usize> {
    if !self.is_list() {
      return Err(Error::InvalidType {
        expected: "list",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked.
    // nix_get_list_size returns the length as c_uint with no error code.
    let len = unsafe {
      sys::nix_get_list_size(self.state.context.as_ptr(), self.inner.as_ptr())
    };

    Ok(len as usize)
  }

  /// Get an element from this list by index.
  ///
  /// Forces evaluation of the element before returning it. Use
  /// [`list_get_lazy`](Self::list_get_lazy) to retrieve an element without
  /// forcing.
  ///
  /// # Arguments
  ///
  /// * `idx` - The index of the element to retrieve (0-based)
  ///
  /// # Returns
  ///
  /// The evaluated list element at `idx`.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a list or the index is out of
  /// bounds.
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use std::sync::Arc;
  /// # use nix_bindings::{Context, EvalStateBuilder, Store};
  /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
  /// # let ctx = Arc::new(Context::new()?);
  /// # let store = Arc::new(Store::open(&ctx, None)?);
  /// # let state = EvalStateBuilder::new(&store)?.build()?;
  /// let list = state.eval_from_string("[1 2 3]", "<eval>")?;
  /// let first = list.list_get(0)?;
  /// assert_eq!(first.as_int()?, 1);
  /// # Ok(())
  /// # }
  /// ```
  pub fn list_get(&self, idx: usize) -> Result<Value<'_>> {
    self.list_get_impl(idx, false)
  }

  /// Get an element from this list by index without forcing evaluation.
  ///
  /// Returns the raw thunk stored at `idx` without evaluating it. The list
  /// itself must already be fully evaluated; only the *element* is returned
  /// unevaluated. This is useful for introspecting or serialising lazy
  /// lists without triggering side-effecting evaluation.
  ///
  /// # Arguments
  ///
  /// * `idx` - The index of the element to retrieve (0-based)
  ///
  /// # Returns
  ///
  /// The list element at `idx`, which may be an unevaluated thunk.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a list or the index is out of
  /// bounds.
  pub fn list_get_lazy(&self, idx: usize) -> Result<Value<'_>> {
    self.list_get_impl(idx, true)
  }

  fn list_get_impl(&self, idx: usize, lazy: bool) -> Result<Value<'_>> {
    if !self.is_list() {
      return Err(Error::InvalidType {
        expected: "list",
        actual:   self.value_type().to_string(),
      });
    }

    let len = self.list_len()?;
    if idx >= len {
      return Err(Error::IndexOutOfBounds {
        index:  idx,
        length: len,
      });
    }

    let c_idx = idx as std::os::raw::c_uint;

    // SAFETY: context, value, and state are valid; index is bounds-checked.
    // Both variants return a GC-owned pointer (refcount incremented).
    // Value's Drop calls nix_value_decref to release our reference.
    let elem_ptr = if lazy {
      unsafe {
        sys::nix_get_list_byidx_lazy(
          self.state.context.as_ptr(),
          self.inner.as_ptr(),
          self.state.as_ptr(),
          c_idx,
        )
      }
    } else {
      unsafe {
        sys::nix_get_list_byidx(
          self.state.context.as_ptr(),
          self.inner.as_ptr(),
          self.state.as_ptr(),
          c_idx,
        )
      }
    };

    let inner = NonNull::new(elem_ptr).ok_or(Error::NullPointer)?;
    Ok(Value {
      inner,
      state: self.state,
    })
  }

  /// Create an iterator over the elements of this list.
  ///
  /// # Errors
  ///
  /// Returns an error if this value is not a list.
  pub fn list_iter(&self) -> Result<ListIterator<'_>> {
    if !self.is_list() {
      return Err(Error::InvalidType {
        expected: "list",
        actual:   self.value_type().to_string(),
      });
    }

    let len = self.list_len()?;
    Ok(ListIterator {
      value:  self,
      index:  0,
      length: len,
    })
  }
}

/// Iterator over elements in a Nix list.
///
/// This struct is created by [`Value::list_iter`] and is used to iterate
/// over the elements of a Nix list.
#[derive(Debug)]
pub struct ListIterator<'a> {
  value:  &'a Value<'a>,
  index:  usize,
  length: usize,
}

impl<'a> Iterator for ListIterator<'a> {
  type Item = Result<Value<'a>>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.index >= self.length {
      return None;
    }

    match self.value.list_get(self.index) {
      Ok(value) => {
        self.index += 1;
        Some(Ok(value))
      },
      Err(e) => {
        self.index += 1;
        Some(Err(e))
      },
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let remaining = self.length - self.index;
    (remaining, Some(remaining))
  }
}

impl ExactSizeIterator for ListIterator<'_> {}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use super::*;
  use crate::{Context, EvalStateBuilder, Store};

  #[test]
  #[serial]
  fn test_is_list() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let list = state
      .eval_from_string("[1 2 3]", "<eval>")
      .expect("Failed to evaluate list");
    assert!(list.is_list());

    let int = state
      .eval_from_string("1", "<eval>")
      .expect("Failed to evaluate int");
    assert!(!int.is_list());
  }

  #[test]
  #[serial]
  fn test_list_len() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let empty = state
      .eval_from_string("[]", "<eval>")
      .expect("Failed to evaluate empty list");
    assert_eq!(empty.list_len().expect("Failed to get list length"), 0);

    let list = state
      .eval_from_string("[1 2 3]", "<eval>")
      .expect("Failed to evaluate list");
    assert_eq!(list.list_len().expect("Failed to get list length"), 3);
  }

  #[test]
  #[serial]
  fn test_list_get() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let list = state
      .eval_from_string("[10 20 30]", "<eval>")
      .expect("Failed to evaluate list");

    let first = list.list_get(0).expect("Failed to get first element");
    assert_eq!(first.as_int().expect("Failed to get int"), 10);

    let second = list.list_get(1).expect("Failed to get second element");
    assert_eq!(second.as_int().expect("Failed to get int"), 20);

    let third = list.list_get(2).expect("Failed to get third element");
    assert_eq!(third.as_int().expect("Failed to get int"), 30);

    // Out of bounds should return IndexOutOfBounds error
    let result = list.list_get(5);
    assert!(matches!(
      result,
      Err(Error::IndexOutOfBounds {
        index:  5,
        length: 3,
      })
    ));
  }

  #[test]
  #[serial]
  fn test_list_get_lazy() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let list = state
      .eval_from_string("[1 (1 + 1) 3]", "<eval>")
      .expect("Failed to evaluate list");

    // Lazy get: second element is a thunk; force it.
    let mut elem = list.list_get_lazy(1).expect("list_get_lazy failed");
    elem.force().expect("force failed");
    assert_eq!(elem.as_int().expect("Failed to get int"), 2);

    // Out of bounds must still error.
    assert!(list.list_get_lazy(10).is_err());
  }

  #[test]
  #[serial]
  fn test_list_iter() {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    let state = EvalStateBuilder::new(&store)
      .expect("Failed to create builder")
      .build()
      .expect("Failed to build state");

    let list = state
      .eval_from_string("[1 2 3]", "<eval>")
      .expect("Failed to evaluate list");

    let mut iter = list.list_iter().expect("Failed to create iterator");
    assert_eq!(iter.len(), 3);

    let first = iter
      .next()
      .expect("Failed to get first")
      .expect("Failed to get first value");
    assert_eq!(first.as_int().expect("Failed to get int"), 1);

    let second = iter
      .next()
      .expect("Failed to get second")
      .expect("Failed to get second value");
    assert_eq!(second.as_int().expect("Failed to get int"), 2);

    let third = iter
      .next()
      .expect("Failed to get third")
      .expect("Failed to get third value");
    assert_eq!(third.as_int().expect("Failed to get int"), 3);

    assert!(iter.next().is_none());
  }
}
