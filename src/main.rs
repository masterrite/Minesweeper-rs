#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use slint::{Image, LogicalSize, ModelRc, SharedPixelBuffer, Timer, TimerMode, VecModel};
use std::{cell::RefCell, rc::Rc, time::{Duration, Instant}};

slint::include_modules!();

// Persisted background lives inside the OS per-app config directory as a single
// copied image file (no stray text file next to the exe, and it survives the
// user moving/deleting the original). We keep a fixed stem and remember the
// extension by globbing for "background.*".
//
// Resolution order, std-only (no extra crate):
//   Windows: %APPDATA%\minesweeper
//   macOS:   $HOME/Library/Application Support/minesweeper
//   Linux:   $XDG_CONFIG_HOME/minesweeper  (or $HOME/.config/minesweeper)
fn config_dir() -> std::path::PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(std::env::temp_dir)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
            .unwrap_or_else(std::env::temp_dir)
    };
    base.join("minesweeper")
}

// Path of the currently-saved background image, if one exists on disk.
fn saved_bg_path() -> Option<std::path::PathBuf> {
    let dir = config_dir();
    for ext in ["png", "jpg", "jpeg", "bmp"] {
        let p = dir.join(format!("background.{ext}"));
        if p.exists() { return Some(p); }
    }
    None
}

// Copy the chosen image into the config dir as "background.<ext>", removing any
// previously-saved background first. Returns the stored path on success.
fn save_bg(src: &std::path::Path) -> Option<std::path::PathBuf> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).ok()?;
    clear_bg(); // remove any prior background.* so only one exists
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
    let dst = dir.join(format!("background.{ext}"));
    std::fs::copy(src, &dst).ok()?;
    Some(dst)
}

// Remove any saved background image from the config dir.
fn clear_bg() {
    let dir = config_dir();
    for ext in ["png", "jpg", "jpeg", "bmp"] {
        let _ = std::fs::remove_file(dir.join(format!("background.{ext}")));
    }
}

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
            // Copy the image into the app config dir so it persists even if the
            // user later moves or deletes the original.
            let stored = save_bg(&path).unwrap_or(path);
            let _ = slint::invoke_from_event_loop(move || {
                if let (Some(img), Some(ui)) = (load_image(&stored), ui_weak.upgrade()) {
                    ui.set_bg_image(img);
                    ui.set_use_bg_image(true);
                }
            });
        }
    });
}

// ── Window size management ────────────────────────────────────────────────────

// Base cell size in logical px. MUST match `cell-base` in main.slint so the
// design (1.0-zoom) window size lines up exactly with the rendered layout.
const CELL_BASE: f32 = 32.0;
// Combined logical height of all the non-grid chrome at zoom 1.0:
// outer padding (8*2) + header (42) + spacing (6) + toolbar (28) + spacing (6).
const CHROME_BASE: f32 = 16.0 + 42.0 + 6.0 + 28.0 + 6.0;

// Design (natural, zoom = 1.0) size of the window for a given difficulty.
fn design_size(diff: Difficulty) -> (f32, f32) {
    let w = (diff.cols as f32 * CELL_BASE) + 16.0; // + horizontal padding
    let h = (diff.rows as f32 * CELL_BASE) + CHROME_BASE;
    (w.max(300.0), h)
}

fn apply_ideal_window_size(ui: &AppWindow, diff: Difficulty) {
    let (w, h) = design_size(diff);
    ui.window().set_size(LogicalSize::new(w, h));
    update_zoom(ui);
}

// Compute ONE scale factor for the whole UI from the window size vs. the
// design size, using min of the two axes so the interface scales uniformly
// (preserving aspect) instead of the grid stretching on its own. Computed in
// Rust to keep `zoom` out of Slint's layout graph and avoid a binding loop.
fn update_zoom(ui: &AppWindow) {
    let size = ui.window().size().to_logical(ui.window().scale_factor());
    let diff = Difficulty {
        cols: ui.get_cols() as usize,
        rows: ui.get_rows() as usize,
        mines: 0,
    };
    let (ref_w, ref_h) = design_size(diff);
    let zoom = (size.width / ref_w).min(size.height / ref_h).clamp(0.5, 4.0);
    ui.set_zoom(zoom);
}


// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let game: Rc<RefCell<Game>> = Rc::new(RefCell::new(Game::new(EASY)));

    apply_ideal_window_size(&ui, EASY);

    // Attempt to restore custom background from the app config dir
    if let Some(path) = saved_bg_path() {
        if let Some(img) = load_image(&path) {
            ui.set_bg_image(img);
            ui.set_use_bg_image(true);
        }
    }

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

    // Keep `zoom` in sync with the window size. Slint has no version-stable
    // per-frame resize callback we can rely on here, so we poll the size on a
    // short interval and only update when the size actually changes. Cheap and
    // responsive. We watch both axes since zoom now depends on width AND height.
    let zoom_timer = Timer::default();
    {
        let ui = ui.as_weak();
        let last = Rc::new(RefCell::new((0.0_f32, 0.0_f32)));
        zoom_timer.start(TimerMode::Repeated, Duration::from_millis(60), move || {
            if let Some(ui) = ui.upgrade() {
                let s = ui.window().size().to_logical(ui.window().scale_factor());
                let (lw, lh) = *last.borrow();
                if (s.width - lw).abs() > 0.5 || (s.height - lh).abs() > 0.5 {
                    *last.borrow_mut() = (s.width, s.height);
                    update_zoom(&ui);
                }
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
      ui.on_clear_bg_image(move || { 
          clear_bg();
          ui2.unwrap().set_use_bg_image(false); 
      }); }

    ui.run()
}