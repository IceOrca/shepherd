#![allow(dead_code)]

cfg_if::cfg_if! {
    if #[cfg(feature = "multi-threaded")] {
        use std::sync::Arc as SharedPtr;
        use std::sync::Weak as WeakPtr;
        cfg_if::cfg_if! {
            if #[cfg(feature = "parking-lot")] {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "rwlock")] {
                        use parking_lot::{RwLock as LockCell, RwLockReadGuard as RefGuard, RwLockWriteGuard as RefGuardMut};
                    } else if #[cfg(feature = "mutex")] {
                        use parking_lot::{Mutex as LockCell, MutexGuard as RefGuard, MutexGuard as RefGuardMut};
                    } else {
                        compile_error!("features 'rwlock' or 'mutex' for parking_lot");
                    }
                }
            } else if #[cfg(feature = "std-sync")] {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "rwlock")] {
                        use std::sync::{RwLock as LockCell, RwLockReadGuard as RefGuard, RwLockWriteGuard as RefGuardMut};
                    } else if #[cfg(feature = "mutex")] {
                        use std::sync::{Mutex as LockCell, MutexGuard as RefGuard, MutexGuard as RefGuardMut};
                    } else {
                        compile_error!("features 'rwlock' or 'mutex' for std-sync");
                    }
                }
            } else {
                compile_error!("features 'parking_lot' or 'std-sync' for multi-threaded");
            }
        }
    } else if #[cfg(feature = "single-threaded")] {
        use std::rc::Rc as SharedPtr;
        use std::rc::Weak as WeakPtr;
        use std::cell::{RefCell as LockCell, Ref as RefGuard, RefMut as RefGuardMut};
    } else {
        compile_error!("features 'multi-threaded' or 'single-threaded'");
    }
}

// use Box as UniquePtr;

cfg_if::cfg_if! {
    if #[cfg(feature = "use-VecDeque-as-childlist")] {
        use std::collections::VecDeque as ChildList;
    } else {
        use std::vec::Vec as ChildList;
    }
}

use internal_hierarchy::TreeNode;
type SharedNode<T> = SharedPtr<TreeNode<T>>;
type WeakNode<T> = WeakPtr<TreeNode<T>>;

mod internal_hierarchy {
    use super::*;

    struct Hierarchy<T> {
        position: usize, /* position at my siblings */
        children: ChildList<SharedNode<T>>,
        #[cfg(feature = "depth-inlined")]
        depth: u16,
    }

    pub(super) struct TreeNode<T> {
        data: LockCell<T>,
        parent: LockCell<WeakPtr<Self>>,
        hier_info: LockCell<Hierarchy<T>>,
    }

    impl<T> TreeNode<T> {
        pub(super) fn new_ptr(
            mydata: T,
            myparent: &WeakPtr<Self>,
            #[cfg(feature = "depth-inlined")] mydepth: u16,
        ) -> SharedPtr<Self> {
            let newnode_ptr: SharedPtr<Self> = SharedPtr::new(TreeNode {
                data: LockCell::new(mydata),
                parent: LockCell::new(WeakPtr::clone(myparent)),
                hier_info: LockCell::new(Hierarchy {
                    position: 0,
                    children: ChildList::new(),
                    #[cfg(feature = "depth-inlined")]
                    depth: mydepth,
                }),
            });

            newnode_ptr
        }

        fn acquire_guard<'a, U>(lockcell: &'a LockCell<U>) -> RefGuard<'a, U>
        where
            U: ?Sized,
        {
            cfg_if::cfg_if! {
                if #[cfg(feature = "multi-threaded")] {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "parking-lot")] {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.read()
                                } else {
                                    lockcell.lock()
                                }
                            }
                        } else {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.read().expect("ntree std RwLock read poisoned")
                                } else {
                                    lockcell.lock().expect("ntree std Mutex lock poisoned")
                                }
                            }
                        }
                    }
                } else {
                    lockcell.borrow()
                }
            }
        }

        fn acquire_guard_mut<'a, U>(lockcell: &'a LockCell<U>) -> RefGuardMut<'a, U>
        where
            U: ?Sized,
        {
            cfg_if::cfg_if! {
                if #[cfg(feature = "multi-threaded")] {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "parking-lot")] {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.write()
                                } else {
                                    lockcell.lock()
                                }
                            }
                        } else {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.write().expect("ntree std RwLock write poisoned")
                                } else {
                                    lockcell.lock().expect("ntree std Mutex lock poisoned")
                                }
                            }
                        }
                    }
                } else {
                    lockcell.borrow_mut()
                }
            }
        }

        fn try_acquire_guard<'a, U>(lockcell: &'a LockCell<U>) -> Option<RefGuard<'a, U>>
        where
            U: ?Sized,
        {
            cfg_if::cfg_if! {
                if #[cfg(feature = "multi-threaded")] {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "parking-lot")] {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.try_read()
                                } else {
                                    lockcell.try_lock()
                                }
                            }
                        } else {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.try_read().ok()
                                } else {
                                    lockcell.try_lock().ok()
                                }
                            }
                        }
                    }
                } else {
                    let res = lockcell.try_borrow();
                    match res {
                        Ok(guard) => Some(guard),
                        Err(_err) => None,
                    }
                }
            }
        }

        fn try_acquire_guard_mut<'a, U>(lockcell: &'a LockCell<U>) -> Option<RefGuardMut<'a, U>>
        where
            U: ?Sized,
        {
            cfg_if::cfg_if! {
                if #[cfg(feature = "multi-threaded")] {
                    cfg_if::cfg_if! {
                        if #[cfg(feature = "parking-lot")] {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.try_write()
                                } else {
                                    lockcell.try_lock()
                                }
                            }
                        } else {
                            cfg_if::cfg_if! {
                                if #[cfg(feature = "rwlock")] {
                                    lockcell.try_write().ok()
                                } else {
                                    lockcell.try_lock().ok()
                                }
                            }
                        }
                    }
                } else {
                    let res = lockcell.try_borrow_mut();
                    match res {
                        Ok(guard) => Some(guard),
                        Err(_err) => None,
                    }
                }
            }
        }

        pub(super) fn parent(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let parent: RefGuard<WeakPtr<Self>> = Self::acquire_guard(&pself.parent);
            WeakPtr::upgrade(&*parent)
        }

        pub(super) fn first_child(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            if children.is_empty() {
                None
            } else {
                Some(SharedPtr::clone(&children[0]))
            }
        }

        pub(super) fn last_child(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            if children.is_empty() {
                None
            } else {
                Some(children[Self::childlist_len(children) - 1].clone())
            }
        }

        pub(super) fn next_sibling(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let parent = Self::parent(pself)?;
            let parent_ctx: RefGuard<Hierarchy<T>> = Self::acquire_guard(&parent.hier_info);
            let siblings: &ChildList<SharedPtr<Self>> = &parent_ctx.children;
            let position: usize = {
                let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
                hier_info.position
            };

            if position < Self::childlist_len(siblings) - 1 {
                Some(SharedPtr::clone(&siblings[position + 1]))
            } else {
                None
            }
        }

        pub(super) fn prev_sibling(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let parent = Self::parent(pself)?;
            let parent_ctx: RefGuard<Hierarchy<T>> = Self::acquire_guard(&parent.hier_info);
            let siblings: &ChildList<SharedPtr<Self>> = &parent_ctx.children;
            let position: usize = {
                let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
                hier_info.position
            };

            if position > 0 {
                Some(SharedPtr::clone(&siblings[position - 1]))
            } else {
                None
            }
        }

        pub(super) fn child_count(pself: &SharedPtr<Self>) -> usize {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            Self::childlist_len(children)
        }

        pub(super) fn position(pself: &SharedPtr<Self>) -> usize {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            hier_info.position
        }

        pub(crate) fn depth(pself: &SharedPtr<Self>) -> u16 {
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
                    hier_info.depth
                } else {
                    let mut depth: u16 = 0;
                    let mut current = Self::parent(pself);

                    while let Some(parent) = current {
                        depth += 1;
                        current = Self::parent(&parent);
                    }
                    depth
                }
            }
        }

        fn childlist_len(childlist: &ChildList<SharedPtr<Self>>) -> usize {
            childlist.len()
        }

        fn childlist_append(childlist: &mut ChildList<SharedPtr<Self>>, child: &SharedPtr<Self>) -> () {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.push_back(SharedPtr::clone(child));
                } else {
                    childlist.push(SharedPtr::clone(child));
                }
            }
        }

        fn childlist_append_front(childlist: &mut ChildList<SharedPtr<Self>>, child: &SharedPtr<Self>) -> () {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.push_front(SharedPtr::clone(child));
                } else {
                    childlist.insert(0, SharedPtr::clone(child));
                }
            }
        }

        fn childlist_insert(
            childlist: &mut ChildList<SharedPtr<Self>>,
            position: usize,
            child: &SharedPtr<Self>,
        ) -> bool {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.insert(position, SharedPtr::clone(child));
                } else {
                    childlist.insert(position, SharedPtr::clone(child));
                }
            }
            true
        }

        fn childlist_remove_at(childlist: &mut ChildList<SharedPtr<Self>>, position: usize) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.remove(position)
                } else {
                    Some(childlist.remove(position))
                }
            }
        }

        fn childlist_pop(childlist: &mut ChildList<SharedPtr<Self>>) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.pop_back()
                } else {
                    childlist.pop()
                }
            }
        }

        fn childlist_pop_front(childlist: &mut ChildList<SharedPtr<Self>>) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "use-VecDeque-as-childlist")] {
                    childlist.pop_front()
                } else {
                    Some(childlist.remove(0))
                }
            }
        }

        pub(super) fn child_at(pself: &SharedPtr<Self>, position: usize) -> Option<SharedPtr<Self>> {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            if position < Self::childlist_len(&children) {
                Some(SharedPtr::clone(&children[position]))
            } else {
                None
            }
        }

        fn reindex_childlist(childlist: &mut ChildList<SharedPtr<Self>>, start_from: usize) -> () {
            for position in start_from..childlist.len() {
                let child: &mut SharedPtr<Self> = &mut childlist[position];
                let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&child.hier_info);
                let hier_info: &mut Hierarchy<T> = &mut *hier_info;

                hier_info.position = position;
            }
        }

        fn set_parent(pself: &SharedPtr<Self>, parent: WeakPtr<Self>) -> () {
            let mut parent_cell: RefGuardMut<WeakPtr<Self>> = Self::acquire_guard_mut(&pself.parent);
            *parent_cell = parent;
        }

        fn detach_subtree_root(pself: &SharedPtr<Self>) -> () {
            Self::set_parent(pself, WeakPtr::new());
            {
                let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                hier_info.position = 0;
            }
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    Self::rebase_depths(pself, 0);
                } else {
                }
            }
        }

        #[cfg(feature = "depth-inlined")]
        fn rebase_depths(pself: &SharedPtr<Self>, depth: u16) -> () {
            let children: Vec<SharedPtr<Self>> = {
                let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                hier_info.depth = depth;
                hier_info.children.iter().cloned().collect()
            };

            for child in children {
                Self::rebase_depths(&child, depth + 1);
            }
        }

        pub(super) fn append_child(pself: &SharedPtr<Self>, child_data: T) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    let depth: u16 = {
                    let hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                        hier_info.depth
                    };
                    let newchild: SharedPtr<Self> =
                    Self::new_ptr(child_data, &SharedPtr::downgrade(pself), depth + 1);
                } else {
                    let newchild: SharedPtr<Self> =
                    Self::new_ptr(child_data, &SharedPtr::downgrade(pself));
                }
            }
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            Self::childlist_append(children, &newchild);
            Self::reindex_childlist(children, Self::childlist_len(children) - 1);

            Some(newchild)
        }

        pub(super) fn append_front_child(pself: &SharedPtr<Self>, child_data: T) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    let depth: u16 = {
                    let hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                        hier_info.depth
                    };
                    let newchild: SharedPtr<Self> =
                    Self::new_ptr(child_data, &SharedPtr::downgrade(pself), depth + 1);
                } else {
                    let newchild: SharedPtr<Self> =
                    Self::new_ptr(child_data, &SharedPtr::downgrade(pself));
                }
            }
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            Self::childlist_append_front(children, &newchild);
            Self::reindex_childlist(children, 0);

            Some(newchild)
        }

        pub(super) fn insert_child_at(
            pself: &SharedPtr<Self>,
            position: usize,
            child_data: T,
        ) -> Option<SharedPtr<Self>> {
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    let depth: u16 = {
                        let hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                        hier_info.depth
                    };
                    let newchild: SharedPtr<Self> =
                        Self::new_ptr(child_data, &SharedPtr::downgrade(pself), depth + 1);
                } else {
                    let newchild: SharedPtr<Self> =
                    Self::new_ptr(child_data, &SharedPtr::downgrade(pself));
                }
            }
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            if position > Self::childlist_len(children) {
                return None;
            }
            children.insert(position, SharedPtr::clone(&newchild));
            Self::reindex_childlist(children, position);
            Some(newchild)
        }

        pub(super) fn pop_child(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let removed_child: Option<SharedPtr<Self>>;
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            if children.is_empty() {
                return None;
            }
            removed_child = Self::childlist_pop(children);
            if let Some(child) = removed_child.as_ref() {
                Self::detach_subtree_root(child);
            }
            removed_child
        }

        pub(super) fn pop_front_child(pself: &SharedPtr<Self>) -> Option<SharedPtr<Self>> {
            let removed_child: Option<SharedPtr<Self>>;
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            if children.is_empty() {
                return None;
            }
            removed_child = Self::childlist_pop_front(children);
            Self::reindex_childlist(children, 0);
            if let Some(child) = removed_child.as_ref() {
                Self::detach_subtree_root(child);
            }
            removed_child
        }

        pub(super) fn remove_child_at(pself: &SharedPtr<Self>, position: usize) -> Option<SharedPtr<Self>> {
            let removed_child: Option<SharedPtr<Self>>;
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            if position >= Self::childlist_len(children) {
                return None;
            }

            removed_child = Self::childlist_remove_at(children, position);
            Self::reindex_childlist(children, position);
            if let Some(child) = removed_child.as_ref() {
                Self::detach_subtree_root(child);
            }
            removed_child
        }

        pub(super) fn remove_child<Fcn>(pself: &SharedPtr<Self>, predicate: &Fcn) -> Option<SharedPtr<Self>>
        where
            Fcn: Fn(&T) -> bool,
        {
            let removed_child: Option<SharedPtr<Self>>;
            let mut hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
            let children: &mut ChildList<SharedPtr<Self>> = &mut hier_info.children;

            let position: usize = children.iter().position(|pchild| {
                let child_data: RefGuardMut<T> = Self::acquire_guard_mut(&pchild.data);
                predicate(&*child_data)
            })?;

            removed_child = Self::childlist_remove_at(children, position);
            Self::reindex_childlist(children, position);
            if let Some(child) = removed_child.as_ref() {
                Self::detach_subtree_root(child);
            }
            removed_child
        }

        pub(super) fn delete(pself: SharedPtr<Self>) -> bool {
            let myparent = Self::parent(&pself);
            if let Some(parent) = myparent {
                let mut parent_ctx: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&parent.hier_info);
                let siblings: &mut ChildList<SharedPtr<Self>> = &mut parent_ctx.children;
                let position: usize = {
                    let hier_info: RefGuardMut<Hierarchy<T>> = Self::acquire_guard_mut(&pself.hier_info);
                    hier_info.position
                };

                if position >= Self::childlist_len(siblings) {
                    return false;
                }
                if !SharedPtr::ptr_eq(&siblings[position], &pself) {
                    return false;
                }

                Self::childlist_remove_at(siblings, position);
                Self::reindex_childlist(siblings, position);
                Self::detach_subtree_root(&pself);
                return true;
            }
            // if the node has no parent, it is a root node, and deleting it means clearing the whole tree
            true
        }

        pub(super) fn find_child<Fcn>(pself: &SharedPtr<Self>, predicate: &Fcn) -> Option<SharedPtr<Self>>
        where
            Fcn: Fn(&T) -> bool,
        {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let hier_info: &Hierarchy<T> = &*hier_info;
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            for pchild in children.iter() {
                let child_data: RefGuard<T> = Self::acquire_guard(&pchild.data);
                if predicate(&*child_data) {
                    return Some(SharedPtr::clone(&pchild));
                }
            }

            None
        }

        pub(super) fn child_position<Fcn>(pself: &SharedPtr<Self>, predicate: &Fcn) -> Option<usize>
        where
            Fcn: Fn(&T) -> bool,
        {
            let hier_info: RefGuard<Hierarchy<T>> = Self::acquire_guard(&pself.hier_info);
            let hier_info: &Hierarchy<T> = &*hier_info;
            let children: &ChildList<SharedPtr<Self>> = &hier_info.children;

            let child_idx = children.iter().position(|pchild| {
                let child_data: RefGuard<T> = Self::acquire_guard(&pchild.data);
                predicate(&*child_data)
            });
            child_idx
        }

        pub(super) fn data<Fcn, V>(pself: &SharedPtr<Self>, get: &Fcn) -> V
        where
            Fcn: Fn(&T) -> V,
        {
            let data: RefGuard<T> = Self::acquire_guard(&pself.data);
            let data: &T = &*data;
            get(data)
        }

        pub(super) fn set_data<Fcn, V>(pself: &SharedPtr<Self>, set: &Fcn, value: &V) -> bool
        where
            Fcn: Fn(&mut T, &V) -> bool,
        {
            let mut data: RefGuardMut<T> = Self::acquire_guard_mut(&pself.data);
            let data: &mut T = &mut *data;
            set(data, value)
        }
    } /* impl<T> TreeNode<T> */
} /* mod internal_hierarchy */

#[derive(Clone)]
pub struct Entry<T> {
    innerobj: SharedNode<T>,
    owner: SharedNode<T>,
}

impl<T> Entry<T> {
    fn new(iter: &SharedNode<T>, owner: &SharedNode<T>) -> Self {
        Self {
            innerobj: SharedPtr::clone(iter),
            owner: SharedPtr::clone(owner),
        }
    }

    pub fn parent(&self) -> Option<Self> {
        let parent: SharedNode<T> = TreeNode::parent(&self.innerobj)?;

        Some(Self {
            innerobj: parent,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn first_child(&self) -> Option<Self> {
        let firstchild_ptr: SharedNode<T> = TreeNode::first_child(&self.innerobj)?;

        Some(Self {
            innerobj: firstchild_ptr,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn last_child(&self) -> Option<Self> {
        let lastchild: SharedNode<T> = TreeNode::last_child(&self.innerobj)?;

        Some(Self {
            innerobj: lastchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn next_sibling(&self) -> Option<Self> {
        let nextsibling: SharedNode<T> = TreeNode::next_sibling(&self.innerobj)?;

        Some(Self {
            innerobj: nextsibling,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn prev_sibling(&self) -> Option<Self> {
        let prevsibling: SharedNode<T> = TreeNode::prev_sibling(&self.innerobj)?;

        Some(Self {
            innerobj: prevsibling,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn child_count(&self) -> usize {
        TreeNode::child_count(&self.innerobj)
    }

    pub fn child_at(&self, position: usize) -> Option<Self> {
        let pchild: SharedNode<T> = TreeNode::child_at(&self.innerobj, position)?;

        Some(Self {
            innerobj: pchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn position(&self) -> usize {
        TreeNode::position(&self.innerobj)
    }

    pub fn depth(&self) -> u16 {
        TreeNode::depth(&self.innerobj)
    }

    pub fn depth_from_owner(&self) -> u16 {
        let mut depth: u16 = 0;
        let mut current = Some(SharedPtr::clone(&self.innerobj));

        while let Some(curr) = current {
            if SharedPtr::ptr_eq(&curr, &self.owner) {
                break;
            }
            depth += 1;
            current = TreeNode::parent(&curr);
        }
        depth
    }

    pub fn child_position<Fcn>(&self, predicate: &Fcn) -> Option<usize>
    where
        Fcn: Fn(&T) -> bool,
    {
        TreeNode::child_position(&self.innerobj, predicate)
    }

    pub fn find_child<Fcn>(&self, predicate: &Fcn) -> Option<Self>
    where
        Fcn: Fn(&T) -> bool,
    {
        let pchild: SharedNode<T> = TreeNode::find_child(&self.innerobj, predicate)?;

        Some(Self {
            innerobj: pchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn append_child(&self, child_data: T) -> Option<Self> {
        let newchild: SharedNode<T> = TreeNode::append_child(&self.innerobj, child_data)?;

        Some(Self {
            innerobj: newchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn append_front_child(&self, child_data: T) -> Option<Self> {
        let newchild: SharedNode<T> = TreeNode::append_front_child(&self.innerobj, child_data)?;

        Some(Self {
            innerobj: newchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn insert_child_at(&self, position: usize, child_data: T) -> Option<Self> {
        let newchild: SharedNode<T> = TreeNode::insert_child_at(&self.innerobj, position, child_data)?;

        Some(Self {
            innerobj: newchild,
            owner: SharedPtr::clone(&self.owner),
        })
    }

    pub fn pop_child(&self) -> Option<Self> {
        let cutchild_ptr: SharedNode<T> = TreeNode::pop_child(&self.innerobj)?;

        Some(Self::new(&cutchild_ptr, &cutchild_ptr))
    }

    pub fn pop_front_child(&self) -> Option<Self> {
        let cutchild_ptr: SharedNode<T> = TreeNode::pop_front_child(&self.innerobj)?;

        Some(Self::new(&cutchild_ptr, &cutchild_ptr))
    }

    pub fn remove_child_at(&self, position: usize) -> Option<Self> {
        let removed_child: SharedNode<T> = TreeNode::remove_child_at(&self.innerobj, position)?;

        Some(Self::new(&removed_child, &removed_child))
    }

    pub fn remove_child<Fcn>(&self, predicate: &Fcn) -> Option<Self>
    where
        Fcn: Fn(&T) -> bool,
    {
        let removed_child: SharedNode<T> = TreeNode::remove_child(&self.innerobj, predicate)?;

        Some(Self::new(&removed_child, &removed_child))
    }

    pub fn delete(self) -> bool {
        TreeNode::delete(self.innerobj)
    }

    pub fn same_owner(&self, other: &Self) -> bool {
        SharedPtr::ptr_eq(&self.owner, &other.owner)
    }

    pub fn data<Fcn, V>(&self, get: &Fcn) -> V
    where
        Fcn: Fn(&T) -> V,
    {
        TreeNode::data(&self.innerobj, get)
    }

    pub fn set_data<Fcn, V>(&self, set: &Fcn, value: &V) -> bool
    where
        Fcn: Fn(&mut T, &V) -> bool,
    {
        TreeNode::set_data(&self.innerobj, set, value)
    }
} /* impl<T> Entry<T> */

impl<T> PartialEq for Entry<T> {
    fn eq(&self, other: &Self) -> bool {
        SharedPtr::ptr_eq(&self.innerobj, &other.innerobj) && SharedPtr::ptr_eq(&self.owner, &other.owner)
    }
}

pub struct Iter<T> {
    curr: Option<SharedNode<T>>,
    owner: Option<SharedNode<T>>,
}

impl<T> Iterator for Iter<T> {
    type Item = Entry<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let curr: &SharedNode<T> = self.curr.as_ref()?;
        let owner: &SharedNode<T> = self.owner.as_ref()?;
        let curr_node = SharedPtr::clone(curr);

        if let Some(child) = TreeNode::first_child(curr) {
            self.curr = Some(child);
        } else if let Some(sibling) = TreeNode::next_sibling(curr) {
            self.curr = Some(sibling);
        } else {
            let mut tmp: SharedPtr<TreeNode<T>> = SharedPtr::clone(curr);
            self.curr = None;

            while let Some(ancestor) = TreeNode::parent(&tmp) {
                if SharedPtr::ptr_eq(&ancestor, owner) {
                    break;
                }

                if let Some(uncle) = TreeNode::next_sibling(&ancestor) {
                    self.curr = Some(uncle);
                    break;
                }
                tmp = ancestor;
            }
        }

        Some(Entry::new(&curr_node, owner))
    }
}

#[derive(Clone, Default)]
pub struct NTree<T> {
    root: Option<SharedNode<T>>,
}

impl<T> NTree<T> {
    pub fn new() -> Self {
        Self { root: None }
    }

    pub fn new_with_root(root_data: T) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let newroot: SharedNode<T> = TreeNode::new_ptr(root_data, &WeakPtr::new(), 0);
            } else {
                let newroot: SharedNode<T> = TreeNode::new_ptr(root_data, &WeakPtr::new());
            }
        }

        Self {
            root: Some(SharedPtr::clone(&newroot)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn new_from(subroot: &Entry<T>) -> Option<Self> {
        Some(Self {
            root: Some(SharedPtr::clone(&subroot.innerobj)),
        })
    }

    pub fn root(&self) -> Option<Entry<T>> {
        if self.is_empty() {
            return None;
        }

        Some(Entry::new(self.root.as_ref()?, self.root.as_ref()?))
    }

    pub fn is_top(&self) -> bool {
        if self.is_empty() {
            return true;
        }
        if let Some(rootnode) = self.root.as_ref() {
            return TreeNode::parent(rootnode).is_none();
        }
        false
    }

    pub fn clear(&mut self) -> bool {
        if let Some(rootnode) = self.root.take() {
            drop(rootnode);
        }
        true
    }

    pub fn append_child(&mut self, child_data: T, parent: &mut Option<Entry<T>>) -> Option<Entry<T>> {
        if self.is_empty() {
            if parent.is_some() {
                // parent is invalid
                return None;
            } else {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "depth-inlined")] {
                        let newroot: SharedNode<T> = TreeNode::new_ptr(child_data, &WeakPtr::new(), 0);
                    } else {
                        let newroot: SharedNode<T> = TreeNode::new_ptr(child_data, &WeakPtr::new());
                    }
                }
                self.root = Some(SharedPtr::clone(&newroot));
                return Some(Entry::new(&newroot, &newroot));
            }
        }

        if let Some(p) = parent.as_mut() {
            if !SharedPtr::ptr_eq(&p.owner, self.root.as_ref()?) {
                return None;
            }
            return p.append_child(child_data);
        }
        // the NTree can not have multiple roots
        None
    }
}

impl<T> IntoIterator for NTree<T> {
    type Item = Entry<T>;
    type IntoIter = Iter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self.root.as_ref() {
            Some(rootnode) => Iter {
                curr: Some(SharedPtr::clone(rootnode)),
                owner: Some(SharedPtr::clone(rootnode)),
            },
            None => Iter {
                curr: None,
                owner: None,
            },
        }
    }
}

impl<'a, T> IntoIterator for &'a NTree<T> {
    type Item = Entry<T>;
    type IntoIter = Iter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self.root.as_ref() {
            Some(rootnode) => Iter {
                curr: Some(SharedPtr::clone(rootnode)),
                owner: Some(SharedPtr::clone(rootnode)),
            },
            None => Iter {
                curr: None,
                owner: None,
            },
        }
    }
}

impl<'a, T> IntoIterator for &'a mut NTree<T> {
    type Item = Entry<T>;
    type IntoIter = Iter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self.root.as_ref() {
            Some(rootnode) => Iter {
                curr: Some(SharedPtr::clone(rootnode)),
                owner: Some(SharedPtr::clone(rootnode)),
            },
            None => Iter {
                curr: None,
                owner: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_value(entry: &Entry<&'static str>) -> &'static str {
        entry.data(&|value| *value)
    }

    #[test]
    fn ntree_append_child_accepts_non_root_parent() {
        let mut tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        let child = root.append_child("child").expect("test: expect Some");
        let mut parent = Some(child.clone());

        let grandchild = tree.append_child("grandchild", &mut parent).expect("test: expect Some");

        assert_eq!(entry_value(&grandchild), "grandchild");
        assert_eq!(child.child_count(), 1);
        assert_eq!(grandchild.parent().map(|entry| entry_value(&entry)), Some("child"));
        assert_eq!(grandchild.depth_from_owner(), 2);
    }

    #[test]
    fn pop_front_child_detaches_and_reindexes_remaining_children() {
        let tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        root.append_child("a").expect("test: expect Some");
        root.append_child("b").expect("test: expect Some");
        root.append_child("c").expect("test: expect Some");

        let detached = root.pop_front_child().expect("test: expect Some");
        let first = root.child_at(0).expect("test: expect Some");
        let second = root.child_at(1).expect("test: expect Some");

        assert_eq!(entry_value(&detached), "a");
        assert!(detached.parent().is_none());
        assert_eq!(detached.position(), 0);
        assert_eq!(detached.depth(), 0);
        assert_eq!(detached.depth_from_owner(), 0);
        assert_eq!(root.child_count(), 2);
        assert_eq!(entry_value(&first), "b");
        assert_eq!(first.position(), 0);
        assert!(first.prev_sibling().is_none());
        assert_eq!(entry_value(&second), "c");
        assert_eq!(second.position(), 1);
        assert_eq!(first.next_sibling().map(|entry| entry_value(&entry)), Some("c"));
    }

    #[test]
    fn remove_child_detaches_subtree_with_new_owner() {
        let tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        let child = root.append_child("child").expect("test: expect Some");
        let grandchild = child.append_child("grandchild").expect("test: expect Some");

        let detached = root.remove_child_at(0).expect("test: expect Some");
        let detached_grandchild = detached.first_child().expect("test: expect Some");

        assert_eq!(entry_value(&detached), "child");
        assert!(detached.parent().is_none());
        assert_eq!(detached.depth(), 0);
        assert_eq!(detached.depth_from_owner(), 0);
        assert_eq!(entry_value(&detached_grandchild), "grandchild");
        assert_eq!(
            detached_grandchild.parent().map(|entry| entry_value(&entry)),
            Some("child")
        );
        assert_eq!(detached_grandchild.depth_from_owner(), 1);
        assert!(!detached.same_owner(&grandchild));
        assert_eq!(root.child_count(), 0);
    }

    #[test]
    fn delete_detaches_entry_and_reindexes_siblings() {
        let tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        let a = root.append_child("a").expect("test: expect Some");
        let b = root.append_child("b").expect("test: expect Some");
        let c = root.append_child("c").expect("test: expect Some");
        let b_after_delete = b.clone();

        assert!(b.delete());

        assert!(b_after_delete.parent().is_none());
        assert_eq!(b_after_delete.position(), 0);
        assert_eq!(root.child_count(), 2);
        assert_eq!(root.child_at(0).map(|entry| entry_value(&entry)), Some("a"));
        assert_eq!(root.child_at(1).map(|entry| entry_value(&entry)), Some("c"));
        assert_eq!(a.position(), 0);
        assert_eq!(c.position(), 1);
    }

    #[test]
    fn iterator_keeps_depth_first_order_after_mutations() {
        let tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        let a = root.append_child("a").expect("test: expect Some");
        a.append_child("a1").expect("test: expect Some");
        root.append_child("b").expect("test: expect Some");

        let values: Vec<&str> = (&tree).into_iter().map(|entry| entry_value(&entry)).collect();

        assert_eq!(values, vec!["root", "a", "a1", "b"]);
    }

    #[test]
    fn subtree_iterator_does_not_escape_owner() {
        let tree = NTree::new_with_root("root");
        let root = tree.root().expect("test: expect Some");
        let a = root.append_child("a").expect("test: expect Some");
        a.append_child("a1").expect("test: expect Some");
        root.append_child("b").expect("test: expect Some");
        let subtree = NTree::new_from(&a).expect("test: expect Some");

        let values: Vec<&str> = (&subtree).into_iter().map(|entry| entry_value(&entry)).collect();

        assert_eq!(values, vec!["a", "a1"]);
    }
} /* mod tests */
