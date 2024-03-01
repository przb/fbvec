use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::{alloc, ptr};
use std::{alloc::Layout, mem, ptr::NonNull};

pub const GROWTH_RATE: usize = 2;

struct RawFbVec<T> {
    ptr: NonNull<T>,
    cap: usize,
}

unsafe impl<T: Send> Send for RawFbVec<T> {}
unsafe impl<T: Sync> Sync for RawFbVec<T> {}

impl<T> RawFbVec<T> {
    fn new() -> Self {
        assert!(mem::size_of::<T>() != 0, "TODO impl ZST support");
        RawFbVec {
            ptr: NonNull::dangling(),
            cap: 0,
        }
    }
    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            1
        } else {
            GROWTH_RATE * self.cap
        };

        let new_layout = Layout::array::<T>(new_cap).unwrap();
        assert!(
            new_layout.size() <= isize::MAX as usize,
            "Allocation too large"
        );

        let new_ptr = if self.cap == 0 {
            unsafe { alloc::alloc(new_layout) }
        } else {
            let old_layout = Layout::array::<T>(self.cap).unwrap();
            let old_ptr = self.ptr.as_ptr() as *mut u8;
            unsafe { alloc::realloc(old_ptr, old_layout, new_layout.size()) }
        };

        self.ptr = match NonNull::new(new_ptr as *mut T) {
            Some(p) => p,
            None => alloc::handle_alloc_error(new_layout),
        };
        self.cap = new_cap;
    }
}
impl<T> Drop for RawFbVec<T> {
    fn drop(&mut self) {
        if self.cap == 0 {
            let layout = Layout::array::<T>(self.cap).unwrap();
            unsafe {
                alloc::dealloc(self.ptr.as_ptr() as *mut u8, layout);
            }
        }
    }
}
pub struct FbVec<T> {
    buf: RawFbVec<T>,
    len: usize,
}

unsafe impl<T: Send> Send for FbVec<T> {}
unsafe impl<T: Sync> Sync for FbVec<T> {}

impl<T> FbVec<T> {
    fn ptr(&self) -> *mut T {
        self.buf.ptr.as_ptr()
    }
    fn cap(&self) -> usize {
        self.buf.cap
    }
    pub fn new() -> Self {
        FbVec {
            buf: RawFbVec::new(),
            len: 0,
        }
    }
    pub fn push(&mut self, elem: T) {
        if self.len == self.cap() {
            self.buf.grow();
        }
        unsafe {
            ptr::write(self.ptr().add(self.len()), elem);
        }
        self.len += 1;
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            None
        } else {
            self.len -= 1;
            unsafe { Some(ptr::read(self.ptr().add(self.len))) }
        }
    }
    pub fn insert(&mut self, index: usize, elem: T) {
        assert!(index <= self.len(), "Index out of bounds");
        if self.len == self.cap() {
            self.buf.grow();
        }
        unsafe {
            ptr::copy(
                self.ptr().add(index),
                self.ptr().add(index + 1),
                self.len - index,
            );
            ptr::write(self.ptr().add(index), elem)
        }
        self.len += 1;
    }
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len, "index out of bounds");
        unsafe {
            self.len -= 1;
            let result = ptr::read(self.ptr().add(index));
            ptr::copy(
                self.ptr().add(index + 1),
                self.ptr().add(index),
                self.len - index,
            );
            result
        }
    }
}

impl<T> Drop for FbVec<T> {
    fn drop(&mut self) {
        while let Some(_) = self.pop() {}
    }
}

impl<T> Deref for FbVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr(), self.len) }
    }
}

impl<T> DerefMut for FbVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr(), self.len) }
    }
}

pub struct IntoIter<T> {
    _buf: RawFbVec<T>,
    start: *const T,
    end: *const T,
}

impl<T> IntoIterator for FbVec<T> {
    type Item = T;

    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        let buf = unsafe { ptr::read(&self.buf) };
        let len = self.len;
        mem::forget(self);
        IntoIter {
            start: buf.ptr.as_ptr(),
            end: if buf.cap == 0 {
                buf.ptr.as_ptr()
            } else {
                unsafe { buf.ptr.as_ptr().add(len) }
            },
            _buf: buf,
        }
    }
}
impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            None
        } else {
            unsafe {
                let result = ptr::read(self.start);
                self.start = self.start.offset(1);
                Some(result)
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = (self.end as usize - self.start as usize) / mem::size_of::<T>();
        (len, Some(len))
    }
}
impl<T> DoubleEndedIterator for IntoIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.start == self.end {
            None
        } else {
            unsafe {
                self.end = self.end.offset(-1);
                Some(ptr::read(self.end))
            }
        }
    }
}
impl<T> Drop for IntoIter<T> {
    fn drop(&mut self) {
        for _ in &mut *self {}
    }
}
