//! Buffer circular de capacidad fija: al llenarse descarta el elemento mas viejo.

use std::collections::VecDeque;

/// Error de construccion del buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    /// Un buffer de capacidad 0 no puede guardar nada.
    ZeroCapacity,
}

/// Buffer con capacidad maxima; conserva el orden de insercion (del mas viejo al mas nuevo).
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    items: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Result<Self, RingError> {
        if capacity == 0 {
            return Err(RingError::ZeroCapacity);
        }
        Ok(Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
        })
    }

    /// Como `new` pero sin error: una capacidad 0 se eleva a 1 (para capacidades constantes).
    pub fn bounded(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            items: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Agrega al final; devuelve el elemento desalojado si estaba lleno.
    pub fn push(&mut self, item: T) -> Option<T> {
        let evicted = if self.items.len() == self.capacity {
            self.items.pop_front()
        } else {
            None
        };
        self.items.push_back(item);
        evicted
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    pub fn last(&self) -> Option<&T> {
        self.items.back()
    }

    /// Conserva solo los elementos que cumplen `keep`, sin alterar el orden.
    pub fn retain(&mut self, keep: impl FnMut(&T) -> bool) {
        self.items.retain(keep);
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
#[path = "ring_tests.rs"]
mod tests;
