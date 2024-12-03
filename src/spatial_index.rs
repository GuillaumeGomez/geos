use std::ffi::c_void;
use std::marker::PhantomData;

use geos_sys::*;

use crate::context_handle::{with_context, PtrWrap};
use crate::Geom;
use crate::{AsRaw, AsRawMut, GResult};

pub trait SpatialIndex<I> {
    fn insert<G: Geom>(&mut self, geometry: &G, item: I);

    fn query<G: Geom, V: FnMut(&I)>(&self, geometry: &G, visitor: V);
}

pub struct STRtree<I> {
    pub(crate) ptr: PtrWrap<*mut GEOSSTRtree>,
    item_type: PhantomData<I>,
}

impl<I> STRtree<I> {
    pub fn with_capacity(node_capacity: usize) -> GResult<STRtree<I>> {
        with_context(|ctx| unsafe {
            let ptr = GEOSSTRtree_create_r(ctx.as_raw(), node_capacity);
            Ok(STRtree {
                ptr: PtrWrap(ptr),
                item_type: PhantomData,
            })
        })
    }

    pub fn iterate<V>(&self, visitor: V)
    where
        V: FnMut(&I),
    {
        with_context(|ctx| unsafe {
            let (closure, callback) = unpack_closure(&visitor);
            GEOSSTRtree_iterate_r(ctx.as_raw(), *self.ptr, Some(callback), closure);
        })
    }
}

impl<I> SpatialIndex<I> for STRtree<I> {
    fn insert<G: Geom>(&mut self, geometry: &G, item: I) {
        with_context(|ctx| unsafe {
            GEOSSTRtree_insert_r(
                ctx.as_raw(),
                *self.ptr,
                geometry.as_raw(),
                Box::into_raw(Box::new(item)) as *mut c_void,
            );
        })
    }

    fn query<'b, G: Geom, V: FnMut(&I)>(&self, geometry: &G, visitor: V) {
        with_context(|ctx| unsafe {
            let (closure, callback) = unpack_closure(&visitor);
            GEOSSTRtree_query_r(
                ctx.as_raw(),
                *self.ptr,
                geometry.as_raw(),
                Some(callback),
                closure,
            );
        })
    }
}

impl<I> AsRaw for STRtree<I> {
    type RawType = GEOSSTRtree;

    fn as_raw(&self) -> *const Self::RawType {
        *self.ptr
    }
}

impl<I> AsRawMut for STRtree<I> {
    type RawType = GEOSSTRtree;

    unsafe fn as_raw_mut_override(&self) -> *mut Self::RawType {
        *self.ptr
    }
}

impl<I> Drop for STRtree<I> {
    fn drop(&mut self) {
        unsafe extern "C" fn callback<I>(item: *mut c_void, _data: *mut c_void) {
            drop(Box::from_raw(item as *mut I));
        }

        with_context(|ctx| unsafe {
            GEOSSTRtree_iterate_r(
                ctx.as_raw(),
                *self.ptr,
                Some(callback::<I>),
                std::ptr::null_mut(),
            );
            GEOSSTRtree_destroy_r(ctx.as_raw(), *self.ptr);
        })
    }
}

unsafe fn unpack_closure<F, I>(
    closure: &F,
) -> (*mut c_void, extern "C" fn(*mut c_void, *mut c_void))
where
    F: FnMut(&I),
{
    extern "C" fn trampoline<F, I>(item: *mut c_void, data: *mut c_void)
    where
        F: FnMut(&I),
    {
        unsafe {
            let closure: &mut F = &mut *(data as *mut F);
            (*closure)(&mut *(item as *mut I));
        }
    }

    (closure as *const F as *mut c_void, trampoline::<F, I>)
}

#[cfg(test)]
mod test {
    use std::collections::HashSet;

    use crate::{Geometry, STRtree, SpatialIndex};

    #[test]
    fn test_strtree() {
        let mut tree = STRtree::<&str>::with_capacity(10).unwrap();

        let point = Geometry::new_from_wkt("POINT(5 5)").unwrap();
        let line = Geometry::new_from_wkt("LINESTRING (0 0, 10 0)").unwrap();
        let polygon = Geometry::new_from_wkt("POLYGON((2 2, 8 2, 8 8, 2 8, 2 2))").unwrap();

        tree.insert(&point, "Point");
        tree.insert(&line, "Line");
        tree.insert(&polygon, "Polygon");

        // Test iterate

        let mut items = HashSet::<&str>::new();
        tree.iterate(|item| {
            items.insert(*item);
        });

        assert_eq!(
            items,
            vec!["Line", "Point", "Polygon"].into_iter().collect()
        );

        // Test query

        items.clear();
        tree.query(&point, |item| {
            items.insert(*item);
        });

        assert_eq!(items, vec!["Point", "Polygon"].into_iter().collect());
    }
}
