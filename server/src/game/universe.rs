use std::fmt;


pub struct Universe {
    width: usize,
    height: usize,
    cells: Vec<bool>,
    buffer: Vec<bool>,
}

impl Universe {
    fn get_index(&self, row: usize, column: usize) -> usize {
        row * self.width + column
    }

    fn live_neighbour_count(&self, row: usize, column: usize) -> u8 {
        let mut count = 0;

        let north = if row == 0 {
            self.height - 1
        } else {
            row - 1
        };
    
        let south = if row == self.height - 1 {
            0
        } else {
            row + 1
        };
    
        let west = if column == 0 {
            self.width - 1
        } else {
            column - 1
        };
    
        let east = if column == self.width - 1 {
            0
        } else {
            column + 1
        };
        unsafe {
            let nw = self.get_index(north, west);
            count += *self.cells.get_unchecked(nw) as u8;
        
            let n = self.get_index(north, column);
            count += *self.cells.get_unchecked(n) as u8;
        
            let ne = self.get_index(north, east);
            count += *self.cells.get_unchecked(ne) as u8;
        
            let w = self.get_index(row, west);
            count += *self.cells.get_unchecked(w) as u8;
        
            let e = self.get_index(row, east);
            count += *self.cells.get_unchecked(e) as u8;
        
            let sw = self.get_index(south, west);
            count += *self.cells.get_unchecked(sw) as u8;
        
            let s = self.get_index(south, column);
            count += *self.cells.get_unchecked(s) as u8;
        
            let se = self.get_index(south, east);
            count += *self.cells.get_unchecked(se) as u8;
        }
        
        count
    }
}

impl fmt::Display for Universe {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (count, cell) in self.cells.iter().enumerate() {
            let symbol = if *cell { '◼' } else { '◻' };
            write!(f, "{}", symbol)?;
            if (count != 0) && (count - 1) % self.width == 0 {
                write!(f, "\n")?;
            }
        }
        Ok(())
    }
}

impl Universe {
    pub fn new() -> Universe {
        let width = 32;
        let height = 32;

        let size = width * height;
        let mut cells = Vec::with_capacity(width * height);
        let mut buffer = Vec::with_capacity(width * height);
        for i in 0..size {
            unsafe { 
                cells.push(i % 2 == 0 || i % 7 == 0);
                buffer.push(i % 2 == 0 || i % 7 == 0);
             }
        }

        Universe {
            width,
            height,
            cells,
            buffer
        }
    }

    pub fn tick(&mut self) {
        // let _timer = Timer::new("Universe::tick");
        for row in 0..self.height {
            for column in 0..self.width {
                let idx = self.get_index(row, column);

                let cell;
                unsafe { 
                    cell = *self.cells.get_unchecked(idx);
                }
                
                let live_neighbours = self.live_neighbour_count(row, column);

                let next_cell = match (cell, live_neighbours) {
                    (true, x) if x < 2 => false,
                    (true, 2) | (true, 3) => true,
                    (true, x) if x > 3 => false,
                    (false, 3) => true,
                    (otherwise, _) => otherwise,
                };
                unsafe {
                    self.buffer[idx] = next_cell;
                }
            }
        }
        std::mem::swap(&mut self.cells, &mut self.buffer);
    }

    pub fn set_width(&mut self, width: usize) {
        self.width = width;
        self.reset();
    }

    pub fn set_height(&mut self, width: usize) {
        self.width = width;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.cells = (0..self.width * self.height).map(|_| false).collect();
    }

    pub fn render(&self) -> String {
        self.to_string()
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn cells(&self) -> *const bool {
        self.cells.as_ptr()
    }

    pub fn toggle_cell(&mut self, row: usize, column: usize) {
        let idx = self.get_index(row, column);
        match self.cells.get(idx) {
            Some(&e) => self.cells[idx] = !e,
            None => (),
        };
    }

    pub fn set_cell_with_wrapping(&mut self, row: usize, column: usize, value: bool) {
        let row = row % self.height;
        let column = column % self.width;
        let idx = self.get_index(row, column);
        match self.cells.get(idx) {
            Some(&e) => self.cells[idx] = value,
            None => (),
        }
    }

    pub fn create_glider(&mut self, row: usize, column: usize) {
        let proto: [((i32, i32), bool); 9] = [
            ((-1, -1), true),
            ((-1, 0), true),
            ((-1, 1), false),
            ((0, -1), true),
            ((0, 0), false),
            ((0, 1), true),
            ((1, -1), true),
            ((1, 0), false),
            ((1, 1), false),
        ];
        for ((dx, dy), value) in proto.iter() {
            self.set_cell_with_wrapping(
                ((row + self.height) as i32 + dx) as usize,
                ((column + self.width) as i32 + dy) as usize,
                *value,
            );
        }
    }

    pub fn create_pulsar(&mut self, row: usize, column: usize) {
        let fills = [-4, -3, -2, 2, 3, 4];

        // clear the area
        for dx in -6..6 {
            for dy in -6..6 {
                self.set_cell_with_wrapping(
                    ((row + self.height) as i32 + dy) as usize,
                    ((column + self.width) as i32 + dx) as usize,
                    false,
                );
            }
        }
        // do horizontal rows
        for dy in [-6, -1, 1, 6].iter() {
            for dx in fills.iter() {
                self.set_cell_with_wrapping(
                    ((row + self.height) as i32 + dy) as usize,
                    ((column + self.width) as i32 + dx) as usize,
                    true,
                );
            }
        }

        // do vertical rows
        for dx in [-6, -1, 1, 6].iter() {
            for dy in fills.iter() {
                self.set_cell_with_wrapping(
                    ((row + self.height) as i32 + dy) as usize,
                    ((column + self.width) as i32 + dx) as usize,
                    true,
                );
            }
        }
    }
}

impl Universe {
    pub fn get_cells(&self) -> &Vec<bool> {
        &self.cells
    }

    pub fn set_cells(&mut self, cells: &[(usize, usize)]) {
        for (row, column) in cells.iter().cloned() {
            let idx = self.get_index(row, column);
            self.cells[idx] = true;
        }
    }
}
