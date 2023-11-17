#[derive(Debug, Clone)]
pub struct IndexedList<T> {
    entries: Vec<Option<T>>,
    free_entries: Vec<usize>,
}

impl<T> IndexedList<T> {
    pub fn new() -> Self {
        IndexedList {
            entries: vec![],
            free_entries: vec![],
        }
    }

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

    pub fn insert_at(&mut self, index: usize, value: T) -> bool {
        if index >= self.entries.len() {
            return false;
        }

        if let Some(_) = self.entries[index] {
            return false;
        }

        self.entries[index] = Some(value);
        true
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

    pub fn iter(&self) -> IndexedListIterator<T> {
        IndexedListIterator(0, self)
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

impl<T> Default for IndexedList<T> {
    fn default() -> Self {
        Self { entries: Vec::new(), free_entries: Vec::new() }
    }
}

pub struct IndexedListIterator<'list, T>(usize, &'list IndexedList<T>);

impl<'list, T> Iterator for IndexedListIterator<'list, T> {
    type Item = Option<&'list T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.1.get(self.0) {
            None => None,
            Some(value) => {
                self.0 += 1;
                Some(Some(value))
            },
        }
    }
}
