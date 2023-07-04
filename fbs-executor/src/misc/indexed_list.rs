
#[derive(Debug, Default)]
pub struct IndexedList<T> {
    entries: Vec<Option<T>>,
    free_entries: Vec<usize>,
}

impl<T> IndexedList<T> {
    pub fn insert(&mut self, value: T) -> usize {
        let index = match self.free_entries.pop() {
            Some(i) => i,
            None => {
                self.entries.push(None);
                self.entries.len() - 1
            }
        };

        self.entries[index] = Some(value);
        index
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.entries.len() {
            return None;
        }

        let value = self.entries[index].take();
        match value {
            None => None,
            Some(_) => {
                self.free_entries.push(index);
                value
            },
        }
    }

    pub fn allocate(&mut self) -> usize {
        match self.free_entries.pop() {
            Some(i) => i,
            None => {
                self.entries.push(None);
                self.entries.len() - 1
            }
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.entries.len() {
            return None;
        }

        self.entries[index].as_ref()
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.entries.len() {
            return None;
        }

        self.entries[index].as_mut()
    }

    pub fn size(&self) -> usize {
        self.entries.len() - self.free_entries.len()
    }
}

impl<T: Clone> IndexedList<T> {
    pub fn clone(&self, index: usize) -> Option<T> {
        if index >= self.entries.len() {
            return None;
        }

        self.entries[index].clone()
    }
}