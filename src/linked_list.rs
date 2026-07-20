use core::cmp::Ordering;

use alloc::boxed::Box;

struct InnerNode<T>
{
    previous: u16,
    pub value: T,
    next: u16
}
pub struct Node<'a, T>
{
    inner: &'a InnerNode<T>
}
impl<'a, T> Node<'a, T>
{
    #[must_use]
    #[inline]
    pub fn into_next(self) -> u16
    {
        return self.inner.next;
    }
    #[must_use]
    #[inline]
    pub fn into_previous(self) -> u16
    {
        return self.inner.previous;
    }
}
impl<'a, T> AsRef<T> for Node<'a, T>
{
    #[inline]
    fn as_ref(&self) -> &T
    {
        return &self.inner.value;
    }
}
pub struct NodeMut<'a, T>
{
    inner: &'a mut InnerNode<T>
}
impl<'a, T> AsMut<T> for NodeMut<'a, T>
{
    #[inline]
    fn as_mut(&mut self) -> &mut T
    {
        return &mut self.inner.value;
    }
}
impl<'a, T> NodeMut<'a, T>
{
    #[must_use]
    #[inline]
    pub fn into_next(self) -> u16
    {
        return self.inner.next;
    }
    #[must_use]
    #[inline]
    pub fn into_previous(self) -> u16
    {
        return self.inner.previous;
    }
}

/// A looping Linked List stored in a fix data block
pub struct LinkedList<T: Copy, const N: usize>
{
    data: [InnerNode<T>; N],
    first: Option<u16>,
    last: Option<u16>,
    len: usize
}

impl<T: Copy, const N: usize> LinkedList<T, N>
{
    #[must_use]
    pub fn new() -> Box<Self>
    {
        let mut r: Box<Self> = unsafe { Box::new_zeroed().assume_init() };
        r.first = None;
        r.last = None;
        r.len = 0;
        return r;
    }
    
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize
    {
        return self.len;
    }
    
    pub fn add_value(&mut self, value: T) -> Option<u16>
    {
        if self.len >= N { return None; }
        
        let ni = self.len as u16;
        self.len += 1;
        self.add_value_index(value, ni);
        return Some(ni);
    }
    fn add_value_index(&mut self, value: T, index: u16)
    {
        let mut node = InnerNode { previous: 0, value, next: 0 };
        
        match self.last
        {
            Some(i) =>
            {
                let cl = &mut self.data[i as usize];
                node.previous = i;
                let next = cl.next;
                node.next = next;
                cl.next = index;
                self.data[index as usize] = node;
                self.data[next as usize].previous = index;
                self.last = Some(index);
            },
            None =>
            {
                self.data[index as usize] = node;
                self.first = Some(index);
                self.last = Some(index);
            },
        }
    }
    
    pub fn sort<F>(&mut self, mut compare: F)
        where F: FnMut(&T, &T) -> Ordering
    {
        self.data[0..self.len].sort_unstable_by(|a, b| compare(&a.value, &b.value));
        // set links to be in order of data
        for (i, n) in self.data[0..self.len].iter_mut().enumerate()
        {
            n.next = ((i as usize + 1) % self.len) as u16;
            n.previous = ((i as isize - 1) % self.len as isize) as u16;
        }
    }
    
    #[must_use]
    pub fn get_value<'a>(&'a self, index: u16) -> &'a T
    {
        // out of bounds
        if self.len <= index as usize { panic!(); }
        return &self.data[index as usize].value;
    }
    #[must_use]
    pub fn get_value_mut<'a>(&'a mut self, index: u16) -> &'a mut T
    {
        // out of bounds
        if self.len <= index as usize { panic!(); }
        return &mut self.data[index as usize].value;
    }
    #[must_use]
    pub fn get_node<'a>(&'a self, index: u16) -> Node<'a, T>
    {
        // out of bounds
        if self.len <= index as usize { panic!(); }
        return Node { inner: &self.data[index as usize] };
    }
    #[must_use]
    pub fn get_node_mut<'a>(&'a mut self, index: u16) -> NodeMut<'a, T>
    {
        // out of bounds
        if self.len <= index as usize { panic!(); }
        return NodeMut { inner: &mut self.data[index as usize] };
    }
    // #[must_use]
    // pub fn iter_mut_from<'a>(&'a mut self, mut index: u16) -> impl Iterator<Item = &'a mut T>
    // {
    //     // out of bounds
    //     if index as usize >= self.len
    //     {
    //         return LinkedIterMut { ll: self, current: 0xFFFF, start: 0xFFFF };
    //     }
        
    //     let mut start = 0xFFFF;
    //     match self.first
    //     {
    //         Some(v) => start = v,
    //         None => index = 0xFFFF,
    //     };
    //     return LinkedIterMut { ll: self, current: index, start };
    // }
    #[must_use]
    pub fn iter_index(&self) -> impl Iterator<Item = (u16, &T)>
    {
        let start = match self.first
        {
            Some(v) => v,
            None => 0xFFFF
        };
        return LinkedIterIndex { ll: self, current: start, start };
    }
    
    pub fn insert_sorted<F>(&mut self, compare: F, value: T) -> Option<u16>
        where F: FnMut(&T, &T) -> Ordering
    {
        if self.len >= N
        {
            return None;
        }
        
        let ni = self.len as u16;
        self.len += 1;
        self.insert_sorted_index(compare, value, ni);
        return Some(ni);
    }
    fn insert_sorted_index<F>(&mut self, mut compare: F, value: T, index: u16)
        where F: FnMut(&T, &T) -> Ordering
    {
        let start = match self.first
        {
            Some(i) => i,
            None => return self.add_value_index(value, index)
        };
        
        // finds the next to the node being added
        let mut current = &mut self.data[start as usize];
        let mut index = 0xFFFF;
        while index != start && compare(&current.value, &value).is_le()
        {
            index = current.next;
            current = &mut self.data[index as usize];
        }
        
        // add to end
        if index == start
        {
            return self.add_value_index(value, index);
        }
        
        let mut node = InnerNode { previous: 0, value, next: 0 };
        
        // set new previouses
        let pi = current.previous;
        node.previous = pi;
        current.previous = index;
        // set new nexts
        let previous = &mut self.data[pi as usize];
        node.next = previous.next;
        previous.next = index;
        
        // is the new first
        if index == 0xFFFF
        {
            self.first = Some(index);
        }
        
        self.data[index as usize] = node;
    }
    /// WARNING - will not be able to fill again unless we add a new value to the index straight away
    fn remove(&mut self, index: u16)
    {
        let node = &mut self.data[index as usize];
        let ni = node.next;
        let pi = node.previous;
        
        self.data[pi as usize].next = ni;
        self.data[ni as usize].previous = pi;
    }
    
    pub fn update_value<F>(&mut self, compare: F, index: u16, value: T) -> bool
        where F: FnMut(&T, &T) -> Ordering
    {
        // out of bounds
        if index as usize >= self.len
        {
            return false;
        }
        // already done
        if self.len <= 1 { return true; }
        
        // temporarily remove - then add again (but without using a new index)
        self.remove(index);
        self.insert_sorted_index(compare, value, index);
        return true;
    }
}

// struct LinkedIterMut<'a, T: Copy, const N: usize>
// {
//     ll: &'a mut LinkedList<T, N>,
//     current: u16,
//     start: u16
// }
// impl<'a, T: Copy, const N: usize> Iterator for LinkedIterMut<'a, T, N>
// {
//     type Item = &'a mut T;

//     fn next(&mut self) -> Option<Self::Item>
//     {
//         let mut current = self.current;
//         if current == 0xFFFF
//         {
//             return None;
//         }
        
//         let node = self.ll.get_node_mut(current).inner;
        
//         current = node.next;
//         if current == self.start
//         {
//             current = 0xFFFF;
//         }
//         self.current = current;
        
//         // force 'a lifetime
//         // is ok as the original data has lifetime 'a and mut here will not be used twice
//         unsafe 
//         {
//             let ptr = node as *mut InnerNode<T>;
//             return Some(&mut ptr.as_mut::<'a>().unwrap().value);
//         }
//     }
// }

struct LinkedIterIndex<'a, T: Copy, const N: usize>
{
    ll: &'a LinkedList<T, N>,
    current: u16,
    start: u16
}
impl<'a, T: Copy, const N: usize> Iterator for LinkedIterIndex<'a, T, N>
{
    type Item = (u16, &'a T);

    fn next(&mut self) -> Option<Self::Item>
    {
        let mut current = self.current;
        if current == 0xFFFF
        {
            return None;
        }
        
        let node = self.ll.get_node(current).inner;
        let index = current;
        
        current = node.next;
        if current == self.start
        {
            current = 0xFFFF;
        }
        self.current = current;
        return Some((index, &node.value));
    }
}