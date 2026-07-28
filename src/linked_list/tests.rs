use core::{cmp::Ordering, mem::MaybeUninit};
use alloc::vec::Vec;

use crate::linked_list::{InnerNode, LinkedList};

fn array_helper<T, const L: usize, const N: usize>(values: [T; L]) -> [T; N]
{
    let mut result = unsafe { MaybeUninit::<[T; N]>::uninit().assume_init() };
    let mut iter = values.into_iter();
    result[..L].fill_with(|| iter.next().unwrap());
    return result;
}

#[test]
fn iteration()
{
    let ll = LinkedList::<usize, 16> {
        data: array_helper([InnerNode { previous: 3, value: 1, next: 4  },
            InnerNode { previous: 4, value: 2, next: 2  },
            InnerNode { previous: 1, value: 3, next: 3  },
            InnerNode { previous: 2, value: 4, next: 0  },
            InnerNode { previous: 0, value: 5, next: 1  }]),
        first: Some(2),
        last: Some(1),
        len: 5
    };
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(2, &3), (3, &4), (0, &1), (4, &5), (1, &2)]);
}

fn cmp(l: &usize, r: &usize) -> Ordering
{
    return l.cmp(r);
}

#[test]
fn insertion()
{
    let mut ll = LinkedList::<usize, 16> {
        data: array_helper([InnerNode { previous: 3, value: 6, next: 4  },
            InnerNode { previous: 4, value: 10, next: 2  },
            InnerNode { previous: 1, value: 2, next: 3  },
            InnerNode { previous: 2, value: 4, next: 0  },
            InnerNode { previous: 0, value: 8, next: 1  }]),
        first: Some(2),
        last: Some(1),
        len: 5
    };
    
    ll.insert_sorted(cmp, 5);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(2, &2), (3, &4), (5, &5), (0, &6), (4, &8), (1, &10)]);
    
    // deals with duplicates
    ll.insert_sorted(cmp, 5);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(2, &2), (3, &4), (5, &5), (6, &5), (0, &6), (4, &8), (1, &10)]);
    
    // check ends
    assert_eq!(ll.first, Some(2));
    assert_eq!(ll.last, Some(1));
}
#[test]
fn insertion_ends()
{
    let mut ll = LinkedList::<usize, 16> {
        data: array_helper([InnerNode { previous: 3, value: 6, next: 4  },
            InnerNode { previous: 4, value: 10, next: 2  },
            InnerNode { previous: 1, value: 2, next: 3  },
            InnerNode { previous: 2, value: 4, next: 0  },
            InnerNode { previous: 0, value: 8, next: 1  }]),
        first: Some(2),
        last: Some(1),
        len: 5
    };
    
    ll.insert_sorted(cmp, 11);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(2, &2), (3, &4), (0, &6), (4, &8), (1, &10), (5, &11)]);
    assert_eq!(ll.first, Some(2));
    assert_eq!(ll.last, Some(5));
    
    ll.insert_sorted(cmp, 1);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(6, &1), (2, &2), (3, &4), (0, &6), (4, &8), (1, &10), (5, &11)]);
    assert_eq!(ll.first, Some(6));
    assert_eq!(ll.last, Some(5));
}

#[test]
fn update()
{
    let mut ll = LinkedList::<usize, 16> {
        data: array_helper([InnerNode { previous: 3, value: 6, next: 4  },
            InnerNode { previous: 4, value: 10, next: 2  },
            InnerNode { previous: 1, value: 2, next: 3  },
            InnerNode { previous: 2, value: 4, next: 0  },
            InnerNode { previous: 0, value: 8, next: 1  }]),
        first: Some(2),
        last: Some(1),
        len: 5
    };
    
    ll.update_value(cmp, 3, 9);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(2, &2), (0, &6), (4, &8), (3, &9), (1, &10)]);
    assert_eq!(ll.first, Some(2));
    assert_eq!(ll.last, Some(1));
}
#[test]
fn update_ends()
{
    let mut ll = LinkedList::<usize, 16> {
        data: array_helper([InnerNode { previous: 3, value: 6, next: 4  },
            InnerNode { previous: 4, value: 10, next: 2  },
            InnerNode { previous: 1, value: 2, next: 3  },
            InnerNode { previous: 2, value: 4, next: 0  },
            InnerNode { previous: 0, value: 8, next: 1  }]),
        first: Some(2),
        last: Some(1),
        len: 5
    };
    
    ll.update_value(cmp, 2, 5);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(3, &4), (2, &5), (0, &6), (4, &8), (1, &10)]);
    assert_eq!(ll.first, Some(3));
    assert_eq!(ll.last, Some(1));
    
    ll.update_value(cmp, 1, 7);
    
    let vec: Vec<_> = ll.iter_index().collect();
    assert_eq!(&vec, &[(3, &4), (2, &5), (0, &6), (1, &7), (4, &8)]);
    assert_eq!(ll.first, Some(3));
    assert_eq!(ll.last, Some(4));
}