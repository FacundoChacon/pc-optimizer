use super::*;

#[test]
fn ring_should_keep_the_last_items_in_order_when_capacity_is_exceeded() {
    let mut ring = RingBuffer::new(3).unwrap();
    assert_eq!(ring.push(1), None);
    assert_eq!(ring.push(2), None);
    assert_eq!(ring.push(3), None);
    assert_eq!(ring.push(4), Some(1));
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    assert_eq!(ring.len(), 3);
}

#[test]
fn ring_should_keep_other_capacity_and_other_items() {
    let mut ring = RingBuffer::new(2).unwrap();
    for c in ['a', 'b', 'c', 'd', 'e'] {
        ring.push(c);
    }
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), vec!['d', 'e']);
    assert_eq!(ring.last(), Some(&'e'));
}

#[test]
fn ring_should_be_empty_before_any_push() {
    let ring: RingBuffer<u8> = RingBuffer::new(4).unwrap();
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());
    assert_eq!(ring.iter().count(), 0);
    assert_eq!(ring.last(), None);
}

#[test]
fn ring_should_reject_zero_capacity_with_a_typed_error() {
    assert_eq!(
        RingBuffer::<u8>::new(0).err(),
        Some(RingError::ZeroCapacity)
    );
}

#[test]
fn ring_retain_should_drop_old_items_and_keep_order() {
    let mut ring = RingBuffer::new(5).unwrap();
    for n in [10, 20, 30, 40] {
        ring.push(n);
    }
    ring.retain(|n| *n >= 30);
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), vec![30, 40]);
    ring.retain(|_| false);
    assert!(ring.is_empty());
}

#[test]
fn ring_clear_should_empty_it_keeping_the_capacity() {
    let mut ring = RingBuffer::new(2).unwrap();
    ring.push(1);
    ring.clear();
    assert!(ring.is_empty());
    assert_eq!(ring.capacity(), 2);
}

#[test]
fn bounded_should_clamp_a_zero_capacity_to_one_instead_of_failing() {
    let mut ring = RingBuffer::bounded(0);
    ring.push(1);
    ring.push(2);
    assert_eq!(ring.capacity(), 1);
    assert_eq!(ring.iter().copied().collect::<Vec<_>>(), vec![2]);
    assert_eq!(RingBuffer::<u8>::bounded(7).capacity(), 7);
}
