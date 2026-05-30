#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use slint::{Image, LogicalSize, ModelRc, SharedPixelBuffer, Timer, TimerMode, VecModel};
use std::{cell::RefCell, rc::Rc, time::{Duration, Instant}};

slint::include_modules!();

// ── Difficulty ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Difficulty { cols: usize, rows: usize, mines: usize }

const EASY:   Difficulty = Difficulty { cols: 9,  rows: 9,  mines: 10 };
const MEDIUM: Difficulty = Difficulty { cols: 16, rows: 16, mines: 40 };
const HARD:   Difficulty = Difficulty { cols: 30, rows: 16, mines: 99 };

fn difficulty(i: i32) -> Difficulty { match i { 0 => EASY, 2 => HARD, _ => MEDIUM } }

// ── Cell ──────────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct CellState {
    is_mine: bool, is_revealed: bool, is_flagged: bool, neighbor_count: u8,
}

// ── Game ──────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Phase { Playing, Won, Lost }

struct Game {
    diff: Difficulty, cells: Vec<CellState>, phase: Phase,
    mines_flagged: i32, pending_init: bool,
    start_time: Option<Instant>, elapsed: u32,
}

impl Game {
    fn new(diff: Difficulty) -> Self {
        Self { diff, cells: vec![CellState::default(); diff.cols * diff.rows],
               phase: Phase::Playing, mines_flagged: 0, pending_init: true,
               start_time: None, elapsed: 0 }
    }

    fn idx(&self, col: usize, row: usize) -> usize { row * self.diff.cols + col }

    fn in_bounds(&self, col: i32, row: i32) -> bool {
        col >= 0 && row >= 0 && col < self.diff.cols as i32 && row < self.diff.rows as i32
    }

    fn neighbors(&self, col: usize, row: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::with_capacity(8);
        for dc in -1i32..=1 { for dr in -1i32..=1 {
            if dc == 0 && dr == 0 { continue; }
            let (nc, nr) = (col as i32 + dc, row as i32 + dr);
            if self.in_bounds(nc, nr) { out.push((nc as usize, nr as usize)); }
        }}
        out
    }

    fn place_mines(&mut self, safe_col: usize, safe_row: usize) {
        let safe_idx = self.idx(safe_col, safe_row);
        let mut safe: std::collections::HashSet<usize> = [safe_idx].into();
        for (nc, nr) in self.neighbors(safe_col, safe_row) { safe.insert(self.idx(nc, nr)); }
        let mut cands: Vec<usize> = (0..self.diff.cols * self.diff.rows)
            .filter(|i| !safe.contains(i)).collect();
        cands.shuffle(&mut SmallRng::from_entropy());
        for &i in cands.iter().take(self.diff.mines) { self.cells[i].is_mine = true; }
        for row in 0..self.diff.rows { for col in 0..self.diff.cols {
            if self.cells[self.idx(col, row)].is_mine { continue; }
            let n = self.neighbors(col, row).iter()
                .filter(|&&(nc, nr)| self.cells[self.idx(nc, nr)].is_mine).count();
            let i = self.idx(col, row);
            self.cells[i].neighbor_count = n as u8;
        }}
    }

    fn reveal(&mut self, start_col: usize, start_row: usize) {
        let start = self.idx(start_col, start_row);
        if self.cells[start].is_revealed || self.cells[start].is_flagged { return; }
        if self.cells[start].is_mine {
            self.cells[start].is_revealed = true;
            self.phase = Phase::Lost;
            for c in &mut self.cells { if c.is_mine { c.is_revealed = true; } }
            return;
        }
        let mut stack = vec![(start_col, start_row)];
        while let Some((col, row)) = stack.pop() {
            let i = self.idx(col, row);
            if self.cells[i].is_revealed || self.cells[i].is_flagged || self.cells[i].is_mine { continue; }
            self.cells[i].is_revealed = true;
            if self.cells[i].neighbor_count == 0 {
                for (nc, nr) in self.neighbors(col, row) {
                    let ni = self.idx(nc, nr);
                    if !self.cells[ni].is_revealed && !self.cells[ni].is_flagged { stack.push((nc, nr)); }
                }
            }
        }
    }

    fn left_click(&mut self, col: usize, row: usize) {
        if self.phase != Phase::Playing { return; }
        let i = self.idx(col, row);
        if self.cells[i].is_revealed || self.cells[i].is_flagged { return; }
        if self.pending_init {
            self.place_mines(col, row);
            self.pending_init = false;
            self.start_time = Some(Instant::now());
        }
        self.reveal(col, row);
        if self.phase == Phase::Playing { self.check_win(); }
    }

    fn right_click(&mut self, col: usize, row: usize) {
        if self.phase != Phase::Playing { return; }
        let i = self.idx(col, row);
        if self.cells[i].is_revealed { return; }
        if self.cells[i].is_flagged { self.cells[i].is_flagged = false; self.mines_flagged -= 1; }
        else                        { self.cells[i].is_flagged = true;  self.mines_flagged += 1; }
    }

    fn chord_click(&mut self, col: usize, row: usize) {
        if self.phase != Phase::Playing { return; }
        let i = self.idx(col, row);
        if !self.cells[i].is_revealed || self.cells[i].neighbor_count == 0 { return; }
        let nbrs = self.neighbors(col, row);
        let flags = nbrs.iter().filter(|&&(nc, nr)| self.cells[self.idx(nc, nr)].is_flagged).count();
        if flags == self.cells[i].neighbor_count as usize {
            let to_reveal: Vec<_> = nbrs.into_iter()
                .filter(|&(nc, nr)| { let j = self.idx(nc, nr); !self.cells[j].is_flagged && !self.cells[j].is_revealed })
                .collect();
            for (nc, nr) in to_reveal { self.reveal(nc, nr); }
            if self.phase == Phase::Playing { self.check_win(); }
        }
    }

    fn check_win(&mut self) {
        if self.cells.iter().filter(|c| !c.is_mine && !c.is_revealed).count() == 0 {
            self.phase = Phase::Won;
        }
    }

    fn mines_remaining(&self) -> i32 { self.diff.mines as i32 - self.mines_flagged }

    fn tick(&mut self) -> u32 {
        if let Some(t) = self.start_time {
            if self.phase == Phase::Playing { self.elapsed = t.elapsed().as_secs() as u32; }
        }
        self.elapsed
    }

    fn to_slint_cells(&self) -> Vec<Cell> {
        self.cells.iter().map(|c| Cell {
            is_mine: c.is_mine, is_revealed: c.is_revealed,
            is_flagged: c.is_flagged, neighbor_count: c.neighbor_count as i32,
        }).collect()
    }

    fn slint_state(&self) -> GameState {
        match self.phase {
            Phase::Playing => GameState::Playing,
            Phase::Won     => GameState::Won,
            Phase::Lost    => GameState::Lost,
        }
    }
}

// ── Image loading ─────────────────────────────────────────────────────────────

fn load_image(path: &std::path::Path) -> Option<Image> {
    use image::ImageReader;
    let data = std::fs::read(path).ok()?;
    let img = ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let (w, h) = (img.width(), img.height());
    let buf = SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(img.as_raw(), w, h);
    Some(Image::from_rgba8(buf))
}

// ── File dialog (async, macOS-safe) ──────────────────────────────────────────

fn open_file_dialog_async(ui_weak: slint::Weak<AppWindow>) {
    let task = rfd::AsyncFileDialog::new()
        .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
        .set_title("Choose background image")
        .pick_file();
    std::thread::spawn(move || {
        if let Some(file) = futures_lite::future::block_on(task) {
            let path = file.path().to_owned();
            let _ = slint::invoke_from_event_loop(move || {
                if let (Some(img), Some(ui)) = (load_image(&path), ui_weak.upgrade()) {
                    ui.set_bg_image(img);
                    ui.set_use_bg_image(true);
                }
            });
        }
    });
}

// ── Window size management ────────────────────────────────────────────────────

fn apply_ideal_window_size(ui: &AppWindow, diff: Difficulty) {
    let cell_size = 35.0; 
    let header_and_ui_height = 140.0;
    let new_width = (diff.cols as f32 * cell_size).max(350.0);
    let new_height = (diff.rows as f32 * cell_size) + header_and_ui_height;
    ui.window().set_size(LogicalSize::new(new_width, new_height));
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let game: Rc<RefCell<Game>> = Rc::new(RefCell::new(Game::new(EASY)));

    apply_ideal_window_size(&ui, EASY);

    let push_state = {
        let ui = ui.as_weak();
        let game = Rc::clone(&game);
        move || {
            let ui = ui.unwrap();
            let g = game.borrow();
            ui.set_cols(g.diff.cols as i32);
            ui.set_rows(g.diff.rows as i32);
            ui.set_mines_remaining(g.mines_remaining());
            ui.set_game_state(g.slint_state());
            ui.set_cells(ModelRc::new(VecModel::from(g.to_slint_cells())));
        }
    };

    push_state();

    // Timer: Simplified to just update elapsed seconds natively at 1hz
    let tick_timer = Timer::default();
    {
        let ui = ui.as_weak();
        let game = Rc::clone(&game);
        tick_timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
            if let Some(ui) = ui.upgrade() {
                let elapsed = game.borrow_mut().tick();
                ui.set_elapsed_seconds(elapsed as i32);
            }
        });
    }

    { let p = push_state.clone(); let g = Rc::clone(&game);
      ui.on_cell_clicked(move |col, row| { g.borrow_mut().left_click(col as usize, row as usize); p(); }); }

    { let p = push_state.clone(); let g = Rc::clone(&game);
      ui.on_cell_flagged(move |col, row| { g.borrow_mut().right_click(col as usize, row as usize); p(); }); }

    { let p = push_state.clone(); let g = Rc::clone(&game);
      ui.on_cell_chorded(move |col, row| { g.borrow_mut().chord_click(col as usize, row as usize); p(); }); }

    { let p = push_state.clone(); let g = Rc::clone(&game);
      let ui2 = ui.as_weak(); 
      ui.on_new_game(move |level| {
          let diff = difficulty(level);
          *g.borrow_mut() = Game::new(diff);
          p();
          
          let ui = ui2.unwrap();
          apply_ideal_window_size(&ui, diff);
      }); }

    { let ui2 = ui.as_weak();
      ui.on_toggle_theme(move || { let u = ui2.unwrap(); u.set_dark_mode(!u.get_dark_mode()); }); }

    { let ui2 = ui.as_weak();
      ui.on_open_bg_picker(move || { open_file_dialog_async(ui2.clone()); }); }

    { let ui2 = ui.as_weak();
      ui.on_clear_bg_image(move || { ui2.unwrap().set_use_bg_image(false); }); }

    ui.run()
}