use std::{ffi::CString, ptr::NonNull};

use crate::{Error, Result, Value};

impl Value<'_> {
  /// Get an attribute by name.
  ///
  /// Returns the value associated with the given attribute name.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not an attribute set or the key
  /// does not exist.
  pub fn get_attr(&self, key: &str) -> Result<Value<'_>> {
    if self.value_type() != crate::ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }

    let key_c = CString::new(key)?;

    // SAFETY: context, value, and state are valid, type is checked
    // nix_get_attr_byname returns an owned (GC-reffed) pointer; we are
    // responsible for decref, which Value's Drop handles.
    let attr_ptr = unsafe {
      crate::sys::nix_get_attr_byname(
        self.state.context.as_ptr(),
        self.inner.as_ptr(),
        self.state.as_ptr(),
        key_c.as_ptr(),
      )
    };

    if attr_ptr.is_null() {
      return Err(Error::KeyNotFound(key.to_string()));
    }

    let inner = NonNull::new(attr_ptr).ok_or(Error::NullPointer)?;
    Ok(Value {
      inner,
      state: self.state,
    })
  }

  /// Get all attribute keys.
  ///
  /// Returns a vector of all attribute names in this attribute set.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not an attribute set.
  pub fn attr_keys(&self) -> Result<Vec<String>> {
    if self.value_type() != crate::ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let count = unsafe {
      crate::sys::nix_get_attrs_size(
        self.state.context.as_ptr(),
        self.inner.as_ptr(),
      )
    };

    let mut keys = Vec::with_capacity(count as usize);

    for i in 0..count {
      // SAFETY: context, value, and state are valid; index is in bounds
      let mut name_ptr: *const std::os::raw::c_char = std::ptr::null();
      // nix_get_attr_byidx also returns an owned value pointer; we don't
      // need the value here, but we must decref it to avoid a leak.
      let val_ptr = unsafe {
        crate::sys::nix_get_attr_byidx(
          self.state.context.as_ptr(),
          self.inner.as_ptr(),
          self.state.as_ptr(),
          i,
          &mut name_ptr,
        )
      };

      // Decref the returned value since we only want the name here.
      if !val_ptr.is_null() {
        unsafe {
          crate::sys::nix_value_decref(self.state.context.as_ptr(), val_ptr);
        }
      }

      if name_ptr.is_null() {
        continue;
      }

      // SAFETY: name_ptr is a valid C string owned by the EvalState
      let name = unsafe {
        std::ffi::CStr::from_ptr(name_ptr)
          .to_string_lossy()
          .into_owned()
      };
      keys.push(name);
    }

    Ok(keys)
  }

  /// Check if an attribute exists.
  ///
  /// Returns true if the attribute set contains the given key.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not an attribute set.
  pub fn has_attr(&self, key: &str) -> Result<bool> {
    if self.value_type() != crate::ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }

    let key_c = CString::new(key)?;

    // SAFETY: context, value, and state are valid, type is checked
    let result = unsafe {
      crate::sys::nix_has_attr_byname(
        self.state.context.as_ptr(),
        self.inner.as_ptr(),
        self.state.as_ptr(),
        key_c.as_ptr(),
      )
    };

    Ok(result)
  }

  /// Create an iterator over key-value pairs.
  ///
  /// # Returns
  ///
  /// An iterator over all key-value pairs in the attribute set.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not an attribute set.
  pub fn attrs(&self) -> Result<AttrIterator<'_>> {
    if self.value_type() != crate::ValueType::Attrs {
      return Err(Error::InvalidType {
        expected: "attrs",
        actual:   self.value_type().to_string(),
      });
    }

    // SAFETY: context and value are valid, type is checked
    let count = unsafe {
      crate::sys::nix_get_attrs_size(
        self.state.context.as_ptr(),
        self.inner.as_ptr(),
      )
    };

    Ok(AttrIterator {
      value: self,
      index: 0,
      count: count as usize,
    })
  }
}

/// Iterator over attribute set key-value pairs.
///
/// This struct provides a way to iterate through all attributes
/// in a Nix attribute set, yielding both the key and value for
/// each attribute.
pub struct AttrIterator<'a> {
  value: &'a Value<'a>,
  index: usize,
  count: usize,
}

impl<'a> Iterator for AttrIterator<'a> {
  // Item lifetime is tied to the source Value's lifetime 'a, not 'static.
  type Item = Result<(String, Value<'a>)>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.index >= self.count {
      return None;
    }

    let idx = self.index;
    self.index += 1;

    // SAFETY: context, value, and state are valid; index is in bounds
    // nix_get_attr_byidx returns an owned (GC-reffed) pointer; Value's
    // Drop will release it.
    let mut name_ptr: *const std::os::raw::c_char = std::ptr::null();
    let attr_ptr = unsafe {
      crate::sys::nix_get_attr_byidx(
        self.value.state.context.as_ptr(),
        self.value.inner.as_ptr(),
        self.value.state.as_ptr(),
        idx as std::os::raw::c_uint,
        &mut name_ptr,
      )
    };

    if attr_ptr.is_null() {
      return Some(Err(Error::NullPointer));
    }

    if name_ptr.is_null() {
      // attr_ptr is GC-reffed; we must release it before returning.
      unsafe {
        crate::sys::nix_value_decref(
          self.value.state.context.as_ptr(),
          attr_ptr,
        );
      }
      return Some(Err(Error::NullPointer));
    }

    // SAFETY: name_ptr is a valid C string owned by the EvalState
    let name = unsafe {
      std::ffi::CStr::from_ptr(name_ptr)
        .to_string_lossy()
        .into_owned()
    };

    // SAFETY: attr_ptr is non-null, verified above
    let inner = unsafe { NonNull::new_unchecked(attr_ptr) };

    let value = Value {
      inner,
      state: self.value.state,
    };

    Some(Ok((name, value)))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    let remaining = self.count - self.index;
    (remaining, Some(remaining))
  }
}

impl ExactSizeIterator for AttrIterator<'_> {}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use serial_test::serial;

  use crate::{Context, EvalStateBuilder, Store};

  fn setup() -> EvalStateBuilder {
    let ctx = Arc::new(Context::new().expect("Failed to create context"));
    let store =
      Arc::new(Store::open(&ctx, None).expect("Failed to open store"));
    EvalStateBuilder::new(&store).expect("Failed to create builder")
  }

  #[test]
  #[serial]
  fn test_get_attr() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{ foo = 42; bar = \"hello\"; }", "<eval>")
      .expect("Failed to evaluate attrs");

    let foo = attrs.get_attr("foo").expect("Failed to get foo");
    assert_eq!(foo.as_int().expect("Failed to get int"), 42);

    let bar = attrs.get_attr("bar").expect("Failed to get bar");
    assert_eq!(bar.as_string().expect("Failed to get string"), "hello");
  }

  #[test]
  #[serial]
  fn test_get_attr_missing() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{ foo = 42; }", "<eval>")
      .expect("Failed to evaluate attrs");

    let result = attrs.get_attr("missing");
    assert!(result.is_err());
  }

  #[test]
  #[serial]
  fn test_attr_keys() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{ foo = 1; bar = 2; baz = 3; }", "<eval>")
      .expect("Failed to evaluate attrs");

    let keys = attrs.attr_keys().expect("Failed to get keys");
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"foo".to_string()));
    assert!(keys.contains(&"bar".to_string()));
    assert!(keys.contains(&"baz".to_string()));
  }

  #[test]
  #[serial]
  fn test_has_attr() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{ foo = 42; }", "<eval>")
      .expect("Failed to evaluate attrs");

    assert!(attrs.has_attr("foo").expect("Failed to check attr"));
    assert!(!attrs.has_attr("bar").expect("Failed to check missing attr"));
  }

  #[test]
  #[serial]
  fn test_attr_iterator() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{ a = 1; b = 2; c = 3; }", "<eval>")
      .expect("Failed to evaluate attrs");

    let iter = attrs.attrs().expect("Failed to create iterator");
    let collected: Vec<_> = iter.collect();

    assert_eq!(collected.len(), 3);
  }

  #[test]
  #[serial]
  fn test_empty_attrs() {
    let state = setup().build().expect("Failed to build state");
    let attrs = state
      .eval_from_string("{}", "<eval>")
      .expect("Failed to evaluate empty attrs");

    let keys = attrs.attr_keys().expect("Failed to get keys");
    assert!(keys.is_empty());

    let has = attrs.has_attr("foo").expect("Failed to check attr");
    assert!(!has);
  }
}
