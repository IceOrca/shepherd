#![allow(dead_code)]

use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

cfg_if::cfg_if! {
    if #[cfg(feature = "use-VecDeque-as-childlist")] {
        use std::collections::VecDeque as ChildList;
    } else {
        use std::vec::Vec as ChildList;
    }
}

type SharedNode<T> = Arc<TreeNode<T>>;
type WeakNode<T> = Weak<TreeNode<T>>;

struct Hierarchy<T> {
    position: usize,
    children: ChildList<SharedNode<T>>,
    #[cfg(feature = "depth-inlined")]
    depth: u16,
}

struct TreeNode<T> {
    data: RwLock<T>,
    parent: RwLock<WeakNode<T>>,
    hier_info: RwLock<Hierarchy<T>>,
}

impl<T> TreeNode<T> {
    fn new_ptr(mydata: T, myparent: &WeakNode<T>, #[cfg(feature = "depth-inlined")] mydepth: u16) -> SharedNode<T> {
        Arc::new(TreeNode {
            data: RwLock::new(mydata),
            parent: RwLock::new(Weak::clone(myparent)),
            hier_info: RwLock::new(Hierarchy {
                position: 0,
                children: ChildList::new(),
                #[cfg(feature = "depth-inlined")]
                depth: mydepth,
            }),
        })
    }

    async fn parent(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let parent = pself.parent.read().await;
        parent.upgrade()
    }

    async fn first_child(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let hier_info = pself.hier_info.read().await;
        Self::childlist_first(&hier_info.children)
    }

    async fn last_child(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let hier_info = pself.hier_info.read().await;
        Self::childlist_last(&hier_info.children)
    }

    async fn next_sibling(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let parent = Self::parent(pself).await?;
        let position = {
            let hier_info = pself.hier_info.read().await;
            hier_info.position
        };
        let parent_ctx = parent.hier_info.read().await;
        let next_position = position.checked_add(1)?;

        parent_ctx.children.get(next_position).cloned()
    }

    async fn prev_sibling(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let parent = Self::parent(pself).await?;
        let position = {
            let hier_info = pself.hier_info.read().await;
            hier_info.position
        };
        let prev_position = position.checked_sub(1)?;
        let parent_ctx = parent.hier_info.read().await;

        parent_ctx.children.get(prev_position).cloned()
    }

    async fn child_count(pself: &SharedNode<T>) -> usize {
        let hier_info = pself.hier_info.read().await;
        hier_info.children.len()
    }

    async fn position(pself: &SharedNode<T>) -> usize {
        let hier_info = pself.hier_info.read().await;
        hier_info.position
    }

    async fn depth(pself: &SharedNode<T>) -> u16 {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let hier_info = pself.hier_info.read().await;
                hier_info.depth
            } else {
                let mut depth: u16 = 0;
                let mut current = Self::parent(pself).await;

                while let Some(parent) = current {
                    depth += 1;
                    current = Self::parent(&parent).await;
                }
                depth
            }
        }
    }

    async fn child_at(pself: &SharedNode<T>, position: usize) -> Option<SharedNode<T>> {
        let hier_info = pself.hier_info.read().await;
        hier_info.children.get(position).cloned()
    }

    fn childlist_first(childlist: &ChildList<SharedNode<T>>) -> Option<SharedNode<T>> {
        childlist.get(0).cloned()
    }

    fn childlist_last(childlist: &ChildList<SharedNode<T>>) -> Option<SharedNode<T>> {
        if childlist.is_empty() {
            None
        } else {
            childlist.get(childlist.len() - 1).cloned()
        }
    }

    fn childlist_push_back(childlist: &mut ChildList<SharedNode<T>>, child: &SharedNode<T>) -> () {
        cfg_if::cfg_if! {
            if #[cfg(feature = "use-VecDeque-as-childlist")] {
                childlist.push_back(Arc::clone(child));
            } else {
                childlist.push(Arc::clone(child));
            }
        }
    }

    fn childlist_push_front(childlist: &mut ChildList<SharedNode<T>>, child: &SharedNode<T>) -> () {
        cfg_if::cfg_if! {
            if #[cfg(feature = "use-VecDeque-as-childlist")] {
                childlist.push_front(Arc::clone(child));
            } else {
                childlist.insert(0, Arc::clone(child));
            }
        }
    }

    fn childlist_pop_back(childlist: &mut ChildList<SharedNode<T>>) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "use-VecDeque-as-childlist")] {
                childlist.pop_back()
            } else {
                childlist.pop()
            }
        }
    }

    fn childlist_pop_front(childlist: &mut ChildList<SharedNode<T>>) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "use-VecDeque-as-childlist")] {
                childlist.pop_front()
            } else {
                if childlist.is_empty() {
                    None
                } else {
                    Some(childlist.remove(0))
                }
            }
        }
    }

    fn childlist_remove_at(childlist: &mut ChildList<SharedNode<T>>, position: usize) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "use-VecDeque-as-childlist")] {
                childlist.remove(position)
            } else {
                if position >= childlist.len() {
                    None
                } else {
                    Some(childlist.remove(position))
                }
            }
        }
    }

    async fn reindex_childlist(childlist: &mut ChildList<SharedNode<T>>, start_from: usize) -> () {
        for position in start_from..childlist.len() {
            if let Some(child) = childlist.get(position) {
                let mut hier_info = child.hier_info.write().await;
                hier_info.position = position;
            }
        }
    }

    async fn set_parent(pself: &SharedNode<T>, parent: WeakNode<T>) -> () {
        let mut parent_cell = pself.parent.write().await;
        *parent_cell = parent;
    }

    async fn detach_subtree_root(pself: &SharedNode<T>) -> () {
        Self::set_parent(pself, Weak::new()).await;
        {
            let mut hier_info = pself.hier_info.write().await;
            hier_info.position = 0;
        }
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                Self::rebase_depths(pself, 0).await;
            } else {
            }
        }
    }

    #[cfg(feature = "depth-inlined")]
    async fn rebase_depths(pself: &SharedNode<T>, depth: u16) -> () {
        let mut stack: Vec<(SharedNode<T>, u16)> = vec![(Arc::clone(pself), depth)];

        while let Some((node, node_depth)) = stack.pop() {
            let children = {
                let mut hier_info = node.hier_info.write().await;
                hier_info.depth = node_depth;
                hier_info.children.iter().cloned().collect::<Vec<_>>()
            };

            for child in children.into_iter().rev() {
                stack.push((child, node_depth + 1));
            }
        }
    }

    async fn append_child(pself: &SharedNode<T>, child_data: T) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let depth = {
                    let hier_info = pself.hier_info.read().await;
                    hier_info.depth
                };
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself), depth + 1);
            } else {
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself));
            }
        }
        let mut hier_info = pself.hier_info.write().await;
        let children = &mut hier_info.children;

        Self::childlist_push_back(children, &newchild);
        Self::reindex_childlist(children, children.len() - 1).await;

        Some(newchild)
    }

    async fn append_front_child(pself: &SharedNode<T>, child_data: T) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let depth = {
                    let hier_info = pself.hier_info.read().await;
                    hier_info.depth
                };
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself), depth + 1);
            } else {
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself));
            }
        }
        let mut hier_info = pself.hier_info.write().await;
        let children = &mut hier_info.children;

        Self::childlist_push_front(children, &newchild);
        Self::reindex_childlist(children, 0).await;

        Some(newchild)
    }

    async fn insert_child_at(pself: &SharedNode<T>, position: usize, child_data: T) -> Option<SharedNode<T>> {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let depth = {
                    let hier_info = pself.hier_info.read().await;
                    hier_info.depth
                };
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself), depth + 1);
            } else {
                let newchild = Self::new_ptr(child_data, &Arc::downgrade(pself));
            }
        }
        let mut hier_info = pself.hier_info.write().await;
        let children = &mut hier_info.children;

        if position > children.len() {
            return None;
        }
        children.insert(position, Arc::clone(&newchild));
        Self::reindex_childlist(children, position).await;
        Some(newchild)
    }

    async fn pop_child(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let removed_child = {
            let mut hier_info = pself.hier_info.write().await;
            Self::childlist_pop_back(&mut hier_info.children)
        };

        if let Some(child) = removed_child.as_ref() {
            Self::detach_subtree_root(child).await;
        }
        removed_child
    }

    async fn pop_front_child(pself: &SharedNode<T>) -> Option<SharedNode<T>> {
        let removed_child = {
            let mut hier_info = pself.hier_info.write().await;
            let removed_child = Self::childlist_pop_front(&mut hier_info.children);
            Self::reindex_childlist(&mut hier_info.children, 0).await;
            removed_child
        };

        if let Some(child) = removed_child.as_ref() {
            Self::detach_subtree_root(child).await;
        }
        removed_child
    }

    async fn remove_child_at(pself: &SharedNode<T>, position: usize) -> Option<SharedNode<T>> {
        let removed_child = {
            let mut hier_info = pself.hier_info.write().await;
            if position >= hier_info.children.len() {
                return None;
            }
            let removed_child = Self::childlist_remove_at(&mut hier_info.children, position);
            Self::reindex_childlist(&mut hier_info.children, position).await;
            removed_child
        };

        if let Some(child) = removed_child.as_ref() {
            Self::detach_subtree_root(child).await;
        }
        removed_child
    }

    async fn remove_child<Fcn>(pself: &SharedNode<T>, predicate: &Fcn) -> Option<SharedNode<T>>
    where
        Fcn: Fn(&T) -> bool,
    {
        let removed_child = {
            let mut hier_info = pself.hier_info.write().await;
            let mut position = None;

            for (index, child) in hier_info.children.iter().enumerate() {
                let child_data = child.data.read().await;
                if predicate(&child_data) {
                    position = Some(index);
                    break;
                }
            }

            let position = position?;
            let removed_child = Self::childlist_remove_at(&mut hier_info.children, position);
            Self::reindex_childlist(&mut hier_info.children, position).await;
            removed_child
        };

        if let Some(child) = removed_child.as_ref() {
            Self::detach_subtree_root(child).await;
        }
        removed_child
    }

    async fn delete(pself: SharedNode<T>) -> bool {
        let myparent = Self::parent(&pself).await;
        if let Some(parent) = myparent {
            let position = {
                let hier_info = pself.hier_info.read().await;
                hier_info.position
            };
            let mut parent_ctx = parent.hier_info.write().await;
            if position >= parent_ctx.children.len() {
                return false;
            }
            if !Arc::ptr_eq(&parent_ctx.children[position], &pself) {
                return false;
            }

            Self::childlist_remove_at(&mut parent_ctx.children, position);
            Self::reindex_childlist(&mut parent_ctx.children, position).await;
            drop(parent_ctx);
            Self::detach_subtree_root(&pself).await;
            return true;
        }
        true
    }

    async fn find_child<Fcn>(pself: &SharedNode<T>, predicate: &Fcn) -> Option<SharedNode<T>>
    where
        Fcn: Fn(&T) -> bool,
    {
        let children = {
            let hier_info = pself.hier_info.read().await;
            hier_info.children.iter().cloned().collect::<Vec<_>>()
        };

        for child in children {
            let child_data = child.data.read().await;
            if predicate(&child_data) {
                return Some(child.clone());
            }
        }

        None
    }

    async fn child_position<Fcn>(pself: &SharedNode<T>, predicate: &Fcn) -> Option<usize>
    where
        Fcn: Fn(&T) -> bool,
    {
        let children = {
            let hier_info = pself.hier_info.read().await;
            hier_info.children.iter().cloned().collect::<Vec<_>>()
        };

        for (index, child) in children.into_iter().enumerate() {
            let child_data = child.data.read().await;
            if predicate(&child_data) {
                return Some(index);
            }
        }

        None
    }

    async fn data<Fcn, V>(pself: &SharedNode<T>, get: Fcn) -> V
    where
        Fcn: FnOnce(&T) -> V,
    {
        let data = pself.data.read().await;
        get(&data)
    }

    async fn set_data<Fcn, V>(pself: &SharedNode<T>, set: Fcn, value: &V) -> bool
    where
        Fcn: FnOnce(&mut T, &V) -> bool,
    {
        let mut data = pself.data.write().await;
        set(&mut data, value)
    }
}

#[derive(Clone)]
pub struct AsyncEntry<T> {
    innerobj: SharedNode<T>,
    owner: SharedNode<T>,
}

impl<T> AsyncEntry<T> {
    fn new(iter: &SharedNode<T>, owner: &SharedNode<T>) -> Self {
        Self {
            innerobj: Arc::clone(iter),
            owner: Arc::clone(owner),
        }
    }

    pub async fn parent(&self) -> Option<Self> {
        let parent = TreeNode::parent(&self.innerobj).await?;
        Some(Self::new(&parent, &self.owner))
    }

    pub async fn first_child(&self) -> Option<Self> {
        let child = TreeNode::first_child(&self.innerobj).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn last_child(&self) -> Option<Self> {
        let child = TreeNode::last_child(&self.innerobj).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn next_sibling(&self) -> Option<Self> {
        let sibling = TreeNode::next_sibling(&self.innerobj).await?;
        Some(Self::new(&sibling, &self.owner))
    }

    pub async fn prev_sibling(&self) -> Option<Self> {
        let sibling = TreeNode::prev_sibling(&self.innerobj).await?;
        Some(Self::new(&sibling, &self.owner))
    }

    pub async fn child_count(&self) -> usize {
        TreeNode::child_count(&self.innerobj).await
    }

    pub async fn child_at(&self, position: usize) -> Option<Self> {
        let child = TreeNode::child_at(&self.innerobj, position).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn position(&self) -> usize {
        TreeNode::position(&self.innerobj).await
    }

    pub async fn depth(&self) -> u16 {
        TreeNode::depth(&self.innerobj).await
    }

    pub async fn depth_from_owner(&self) -> u16 {
        let mut depth: u16 = 0;
        let mut current = Some(Arc::clone(&self.innerobj));

        while let Some(curr) = current {
            if Arc::ptr_eq(&curr, &self.owner) {
                break;
            }
            depth += 1;
            current = TreeNode::parent(&curr).await;
        }
        depth
    }

    pub async fn child_position<Fcn>(&self, predicate: Fcn) -> Option<usize>
    where
        Fcn: Fn(&T) -> bool,
    {
        TreeNode::child_position(&self.innerobj, &predicate).await
    }

    pub async fn find_child<Fcn>(&self, predicate: Fcn) -> Option<Self>
    where
        Fcn: Fn(&T) -> bool,
    {
        let child = TreeNode::find_child(&self.innerobj, &predicate).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn append_child(&self, child_data: T) -> Option<Self> {
        let child = TreeNode::append_child(&self.innerobj, child_data).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn append_front_child(&self, child_data: T) -> Option<Self> {
        let child = TreeNode::append_front_child(&self.innerobj, child_data).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn insert_child_at(&self, position: usize, child_data: T) -> Option<Self> {
        let child = TreeNode::insert_child_at(&self.innerobj, position, child_data).await?;
        Some(Self::new(&child, &self.owner))
    }

    pub async fn pop_child(&self) -> Option<Self> {
        let child = TreeNode::pop_child(&self.innerobj).await?;
        Some(Self::new(&child, &child))
    }

    pub async fn pop_front_child(&self) -> Option<Self> {
        let child = TreeNode::pop_front_child(&self.innerobj).await?;
        Some(Self::new(&child, &child))
    }

    pub async fn remove_child_at(&self, position: usize) -> Option<Self> {
        let child = TreeNode::remove_child_at(&self.innerobj, position).await?;
        Some(Self::new(&child, &child))
    }

    pub async fn remove_child<Fcn>(&self, predicate: Fcn) -> Option<Self>
    where
        Fcn: Fn(&T) -> bool,
    {
        let child = TreeNode::remove_child(&self.innerobj, &predicate).await?;
        Some(Self::new(&child, &child))
    }

    pub async fn delete(self) -> bool {
        TreeNode::delete(self.innerobj).await
    }

    pub fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
    }

    pub async fn data<Fcn, V>(&self, get: Fcn) -> V
    where
        Fcn: FnOnce(&T) -> V,
    {
        TreeNode::data(&self.innerobj, get).await
    }

    pub async fn set_data<Fcn, V>(&self, set: Fcn, value: &V) -> bool
    where
        Fcn: FnOnce(&mut T, &V) -> bool,
    {
        TreeNode::set_data(&self.innerobj, set, value).await
    }
}

impl<T> PartialEq for AsyncEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.innerobj, &other.innerobj) && Arc::ptr_eq(&self.owner, &other.owner)
    }
}

pub struct AsyncIter<T> {
    curr: Option<SharedNode<T>>,
    owner: Option<SharedNode<T>>,
}

impl<T> AsyncIter<T> {
    pub async fn next(&mut self) -> Option<AsyncEntry<T>> {
        let curr = self.curr.as_ref()?;
        let owner = self.owner.as_ref()?;
        let curr_node = Arc::clone(curr);

        if let Some(child) = TreeNode::first_child(curr).await {
            self.curr = Some(child);
        } else if let Some(sibling) = TreeNode::next_sibling(curr).await {
            self.curr = Some(sibling);
        } else {
            let mut tmp = Arc::clone(curr);
            self.curr = None;

            while let Some(ancestor) = TreeNode::parent(&tmp).await {
                if Arc::ptr_eq(&ancestor, owner) {
                    break;
                }

                if let Some(uncle) = TreeNode::next_sibling(&ancestor).await {
                    self.curr = Some(uncle);
                    break;
                }
                tmp = ancestor;
            }
        }

        Some(AsyncEntry::new(&curr_node, owner))
    }
}

#[derive(Clone, Default)]
pub struct AsyncNTree<T> {
    root: Option<SharedNode<T>>,
}

impl<T> AsyncNTree<T> {
    pub fn new() -> Self {
        Self { root: None }
    }

    pub fn new_with_root(root_data: T) -> Self {
        cfg_if::cfg_if! {
            if #[cfg(feature = "depth-inlined")] {
                let newroot = TreeNode::new_ptr(root_data, &Weak::new(), 0);
            } else {
                let newroot = TreeNode::new_ptr(root_data, &Weak::new());
            }
        }

        Self {
            root: Some(Arc::clone(&newroot)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn new_from(subroot: &AsyncEntry<T>) -> Option<Self> {
        Some(Self {
            root: Some(Arc::clone(&subroot.innerobj)),
        })
    }

    pub fn root(&self) -> Option<AsyncEntry<T>> {
        let root = self.root.as_ref()?;
        Some(AsyncEntry::new(root, root))
    }

    pub async fn is_top(&self) -> bool {
        match self.root.as_ref() {
            Some(rootnode) => TreeNode::parent(rootnode).await.is_none(),
            None => true,
        }
    }

    pub fn clear(&mut self) -> bool {
        self.root.take();
        true
    }

    pub async fn append_child(&mut self, child_data: T, parent: &mut Option<AsyncEntry<T>>) -> Option<AsyncEntry<T>> {
        if self.is_empty() {
            if parent.is_some() {
                return None;
            }
            cfg_if::cfg_if! {
                if #[cfg(feature = "depth-inlined")] {
                    let newroot = TreeNode::new_ptr(child_data, &Weak::new(), 0);
                } else {
                    let newroot = TreeNode::new_ptr(child_data, &Weak::new());
                }
            }
            self.root = Some(Arc::clone(&newroot));
            return Some(AsyncEntry::new(&newroot, &newroot));
        }

        if let Some(p) = parent.as_mut() {
            if !Arc::ptr_eq(&p.owner, self.root.as_ref()?) {
                return None;
            }
            return p.append_child(child_data).await;
        }
        None
    }

    pub fn iter(&self) -> AsyncIter<T> {
        match self.root.as_ref() {
            Some(rootnode) => AsyncIter {
                curr: Some(Arc::clone(rootnode)),
                owner: Some(Arc::clone(rootnode)),
            },
            None => AsyncIter {
                curr: None,
                owner: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn entry_value(entry: &AsyncEntry<&'static str>) -> &'static str {
        entry.data(|value| *value).await
    }

    #[tokio::test]
    async fn async_tree_supports_nested_append_and_iteration() {
        let mut tree = AsyncNTree::new_with_root("root");
        let root = tree.root().unwrap();
        let child = root.append_child("child").await.unwrap();
        let mut parent = Some(child.clone());
        let grandchild = tree.append_child("grandchild", &mut parent).await.unwrap();

        assert_eq!(entry_value(&grandchild).await, "grandchild");
        assert_eq!(child.child_count().await, 1);
        let grandchild_parent = grandchild.parent().await.unwrap();
        assert_eq!(entry_value(&grandchild_parent).await, "child");

        let mut iter = tree.iter();
        let mut values = Vec::new();
        while let Some(entry) = iter.next().await {
            values.push(entry_value(&entry).await);
        }
        assert_eq!(values, vec!["root", "child", "grandchild"]);
    }

    #[tokio::test]
    async fn async_remove_detaches_and_reindexes() {
        let tree = AsyncNTree::new_with_root("root");
        let root = tree.root().unwrap();
        root.append_child("a").await.unwrap();
        root.append_child("b").await.unwrap();
        root.append_child("c").await.unwrap();

        let detached = root.pop_front_child().await.unwrap();
        let first = root.child_at(0).await.unwrap();
        let second = root.child_at(1).await.unwrap();

        assert_eq!(entry_value(&detached).await, "a");
        assert!(detached.parent().await.is_none());
        assert_eq!(detached.position().await, 0);
        assert_eq!(detached.depth().await, 0);
        assert_eq!(root.child_count().await, 2);
        assert_eq!(entry_value(&first).await, "b");
        assert_eq!(first.position().await, 0);
        assert!(first.prev_sibling().await.is_none());
        assert_eq!(entry_value(&second).await, "c");
        assert_eq!(second.position().await, 1);
    }

    #[tokio::test]
    async fn async_subtree_iterator_does_not_escape_owner() {
        let tree = AsyncNTree::new_with_root("root");
        let root = tree.root().unwrap();
        let a = root.append_child("a").await.unwrap();
        a.append_child("a1").await.unwrap();
        root.append_child("b").await.unwrap();
        let subtree = AsyncNTree::new_from(&a).unwrap();

        let mut iter = subtree.iter();
        let mut values = Vec::new();
        while let Some(entry) = iter.next().await {
            values.push(entry_value(&entry).await);
        }

        assert_eq!(values, vec!["a", "a1"]);
    }
}
