#![windows_subsystem = "windows"]
#![allow(unused_must_use)]

use std::collections::HashSet;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::Controls::*,
        UI::WindowsAndMessaging::*,
    },
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn get_window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd) as usize;
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; len + 1];
        GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..len])
    }
}

// ── Turing Machine Types ────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Symbol {
    Zero,
    One,
    Blank,
}

impl Symbol {
    fn display(&self) -> &str {
        match self {
            Symbol::Zero => "0",
            Symbol::One => "1",
            Symbol::Blank => "_",
        }
    }

    #[allow(dead_code)]
    fn from_str(s: &str) -> Option<Symbol> {
        match s.trim() {
            "0" => Some(Symbol::Zero),
            "1" => Some(Symbol::One),
            "_" | "" => Some(Symbol::Blank),
            _ => None,
        }
    }

    fn index(&self) -> i32 {
        match self {
            Symbol::Zero => 0,
            Symbol::One => 1,
            Symbol::Blank => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Direction {
    Left,
    Right,
}

impl Direction {
    fn display(&self) -> &str {
        match self {
            Direction::Left => "L",
            Direction::Right => "R",
        }
    }

    fn index(&self) -> i32 {
        match self {
            Direction::Left => 0,
            Direction::Right => 1,
        }
    }
}

#[derive(Clone, Debug)]
struct Transition {
    current_state: String,
    read_symbol: Symbol,
    new_state: String,
    write_symbol: Symbol,
    direction: Direction,
    has_breakpoint: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum RunStatus {
    Idle,
    Running,
    Accepted,
    Rejected,
}

impl RunStatus {
    fn display(&self) -> &str {
        match self {
            RunStatus::Idle => "Idle",
            RunStatus::Running => "Running",
            RunStatus::Accepted => "Accepted",
            RunStatus::Rejected => "Rejected",
        }
    }
}

struct TuringMachine {
    tape: Vec<Symbol>,
    tape_offset: i64,
    head_pos: i64,
    current_state: String,
    start_state: String,
    accept_state: String,
    reject_state: String,
    transitions: Vec<Transition>,
    state_breakpoints: HashSet<String>,
    step_count: u64,
    status: RunStatus,
    timer_speed_ms: u32,
    ui_font: HFONT,
    bold_font: HFONT,

    // Control handles
    h_listview: HWND,
    h_edit_cur_state: HWND,
    h_combo_read: HWND,
    h_edit_new_state: HWND,
    h_combo_write: HWND,
    h_combo_dir: HWND,
    h_status_label: HWND,
    h_speed_trackbar: HWND,
    h_state_bp_edit: HWND,
}

impl TuringMachine {
    fn new() -> Self {
        let tape = vec![Symbol::Blank; 101];
        // tape index 50 corresponds to position 0
        let tape_offset = -50;
        TuringMachine {
            tape,
            tape_offset,
            head_pos: 0,
            current_state: "q0".to_string(),
            start_state: "q0".to_string(),
            accept_state: "qa".to_string(),
            reject_state: "qr".to_string(),
            transitions: Vec::new(),
            state_breakpoints: HashSet::new(),
            step_count: 0,
            status: RunStatus::Idle,
            timer_speed_ms: 500,
            ui_font: HFONT::default(),
            bold_font: HFONT::default(),
            h_listview: HWND::default(),
            h_edit_cur_state: HWND::default(),
            h_combo_read: HWND::default(),
            h_edit_new_state: HWND::default(),
            h_combo_write: HWND::default(),
            h_combo_dir: HWND::default(),
            h_status_label: HWND::default(),
            h_speed_trackbar: HWND::default(),
            h_state_bp_edit: HWND::default(),
        }
    }

    fn tape_index(&self, pos: i64) -> usize {
        (pos - self.tape_offset) as usize
    }

    fn ensure_tape(&mut self, pos: i64) {
        let idx = pos - self.tape_offset;
        if idx < 0 {
            let extra = (-idx) as usize;
            let mut prefix = vec![Symbol::Blank; extra];
            prefix.append(&mut self.tape);
            self.tape = prefix;
            self.tape_offset -= extra as i64;
        } else if idx as usize >= self.tape.len() {
            self.tape.resize(idx as usize + 1, Symbol::Blank);
        }
    }

    fn read_tape(&mut self) -> Symbol {
        self.ensure_tape(self.head_pos);
        self.tape[self.tape_index(self.head_pos)]
    }

    fn write_tape(&mut self, sym: Symbol) {
        self.ensure_tape(self.head_pos);
        let idx = self.tape_index(self.head_pos);
        self.tape[idx] = sym;
    }

    fn find_transition(&self, state: &str, sym: Symbol) -> Option<usize> {
        self.transitions
            .iter()
            .position(|t| t.current_state == state && t.read_symbol == sym)
    }

    fn step(&mut self) -> bool {
        if self.status == RunStatus::Accepted || self.status == RunStatus::Rejected {
            return false;
        }
        if self.current_state == self.accept_state {
            self.status = RunStatus::Accepted;
            return false;
        }
        if self.current_state == self.reject_state {
            self.status = RunStatus::Rejected;
            return false;
        }

        let sym = self.read_tape();
        if let Some(idx) = self.find_transition(&self.current_state.clone(), sym) {
            let t = self.transitions[idx].clone();
            self.write_tape(t.write_symbol);
            self.current_state = t.new_state;
            match t.direction {
                Direction::Left => self.head_pos -= 1,
                Direction::Right => self.head_pos += 1,
            }
            self.step_count += 1;

            // Check accept/reject after step
            if self.current_state == self.accept_state {
                self.status = RunStatus::Accepted;
                return false;
            }
            if self.current_state == self.reject_state {
                self.status = RunStatus::Rejected;
                return false;
            }

            // Check breakpoints
            if t.has_breakpoint || self.state_breakpoints.contains(&self.current_state) {
                return false; // Signal to pause
            }
            true
        } else {
            // No transition found → reject
            self.status = RunStatus::Rejected;
            false
        }
    }

    fn reset(&mut self) {
        self.tape = vec![Symbol::Blank; 101];
        self.tape_offset = -50;
        self.head_pos = 0;
        self.current_state = self.start_state.clone();
        self.step_count = 0;
        self.status = RunStatus::Idle;
    }
}

// ── Control IDs ─────────────────────────────────────────────────────────────

const ID_LISTVIEW: i32 = 1000;
const ID_EDIT_CUR_STATE: i32 = 1001;
const ID_COMBO_READ: i32 = 1002;
const ID_EDIT_NEW_STATE: i32 = 1003;
const ID_COMBO_WRITE: i32 = 1004;
const ID_COMBO_DIR: i32 = 1005;
const ID_BTN_ADD: i32 = 1010;
const ID_BTN_UPDATE: i32 = 1011;
const ID_BTN_DELETE: i32 = 1012;
const ID_BTN_STEP: i32 = 1020;
const ID_BTN_RUN: i32 = 1021;
const ID_BTN_STOP: i32 = 1022;
const ID_BTN_RESET: i32 = 1023;
const ID_BTN_TOGGLE_BP: i32 = 1024;
const ID_TRACKBAR: i32 = 1030;
const ID_STATE_BP_EDIT: i32 = 1031;
const ID_BTN_ADD_STATE_BP: i32 = 1032;
const ID_STATUS_LABEL: i32 = 1040;
const ID_TIMER: usize = 9001;

// ── Custom Draw structures ──────────────────────────────────────────────────

#[repr(C)]
struct NMLVCUSTOMDRAW {
    nmcd: NMCUSTOMDRAW,
    clr_text: COLORREF,
    clr_text_bk: COLORREF,
    i_sub_item: i32,
    dw_item_type: u32,
    clr_face: COLORREF,
    i_icon_effect: i32,
    i_icon_phase: i32,
    i_part_id: i32,
    i_state_id: i32,
    rc_text: RECT,
    u_align: u32,
}

// ── Window Procedure ────────────────────────────────────────────────────────

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let tm_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TuringMachine;

    match msg {
        WM_PAINT => {
            if tm_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let tm = &mut *tm_ptr;
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            // Draw tape area (y=5..80)
            let tape_y = 20;
            let cell_w = 32;
            let cell_h = 32;
            let num_cells = 28;
            let start_x = 20;

            // Select Segoe UI font into DC
            let old_font = SelectObject(hdc, tm.ui_font);

            // State/step info text
            SetBkMode(hdc, TRANSPARENT);
            let info = format!(
                "State: {}   Step: {}   Status: {}",
                tm.current_state,
                tm.step_count,
                tm.status.display()
            );
            let info_w = to_wide(&info);
            TextOutW(hdc, start_x, 5, &info_w[..info_w.len() - 1]);

            // Draw cells
            let half = num_cells / 2;
            for i in 0..num_cells {
                let tape_pos = tm.head_pos - half as i64 + i as i64;
                tm.ensure_tape(tape_pos);
                let sym = tm.tape[tm.tape_index(tape_pos)];

                let x = start_x + i * cell_w;
                let y = tape_y;

                let is_head = tape_pos == tm.head_pos;

                // Background
                if is_head {
                    let brush = CreateSolidBrush(COLORREF(0x00FFFF)); // Yellow (BGR)
                    let rc = RECT {
                        left: x,
                        top: y,
                        right: x + cell_w,
                        bottom: y + cell_h,
                    };
                    FillRect(hdc, &rc, brush);
                    let _ = DeleteObject(brush);
                }

                // Border
                let rc = RECT {
                    left: x,
                    top: y,
                    right: x + cell_w,
                    bottom: y + cell_h,
                };
                DrawEdge(hdc, &rc as *const RECT as *mut RECT, BDR_SUNKENINNER, BF_RECT);

                // Symbol text
                let sym_w = to_wide(sym.display());
                let mut text_rc = RECT {
                    left: x,
                    top: y,
                    right: x + cell_w,
                    bottom: y + cell_h,
                };
                DrawTextW(
                    hdc,
                    &mut sym_w[..sym_w.len() - 1].to_vec(),
                    &mut text_rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );

                // Position label below cell
                let pos_str = format!("{}", tape_pos);
                let pos_w = to_wide(&pos_str);
                TextOutW(hdc, x + 4, y + cell_h + 2, &pos_w[..pos_w.len() - 1]);
            }

            SelectObject(hdc, old_font);
            EndPaint(hwnd, &ps);
            return LRESULT(0);
        }

        WM_COMMAND => {
            if tm_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let tm = &mut *tm_ptr;
            let cmd = (wparam.0 & 0xffff) as i32;
            let notification = ((wparam.0 >> 16) & 0xffff) as u32;

            match cmd {
                ID_BTN_ADD => {
                    if let Some(t) = read_transition_from_editor(tm) {
                        // Check for duplicate
                        if tm
                            .find_transition(&t.current_state, t.read_symbol)
                            .is_none()
                        {
                            tm.transitions.push(t);
                            refresh_listview(tm);
                        }
                    }
                }
                ID_BTN_UPDATE => {
                    let sel = get_listview_selection(tm.h_listview);
                    if sel >= 0 {
                        if let Some(t) = read_transition_from_editor(tm) {
                            tm.transitions[sel as usize] = t;
                            refresh_listview(tm);
                        }
                    }
                }
                ID_BTN_DELETE => {
                    let sel = get_listview_selection(tm.h_listview);
                    if sel >= 0 && (sel as usize) < tm.transitions.len() {
                        tm.transitions.remove(sel as usize);
                        refresh_listview(tm);
                    }
                }
                ID_BTN_STEP => {
                    if tm.status == RunStatus::Idle || tm.status == RunStatus::Running {
                        tm.status = RunStatus::Idle;
                        KillTimer(hwnd, ID_TIMER);
                        tm.step();
                        update_status(tm);
                        InvalidateRect(hwnd, None, true);
                    }
                }
                ID_BTN_RUN => {
                    if tm.status != RunStatus::Accepted && tm.status != RunStatus::Rejected {
                        tm.status = RunStatus::Running;
                        SetTimer(hwnd, ID_TIMER, tm.timer_speed_ms, None);
                        update_status(tm);
                    }
                }
                ID_BTN_STOP => {
                    KillTimer(hwnd, ID_TIMER);
                    if tm.status == RunStatus::Running {
                        tm.status = RunStatus::Idle;
                    }
                    update_status(tm);
                }
                ID_BTN_RESET => {
                    KillTimer(hwnd, ID_TIMER);
                    tm.reset();
                    update_status(tm);
                    InvalidateRect(hwnd, None, true);
                }
                ID_BTN_TOGGLE_BP => {
                    let sel = get_listview_selection(tm.h_listview);
                    if sel >= 0 && (sel as usize) < tm.transitions.len() {
                        tm.transitions[sel as usize].has_breakpoint =
                            !tm.transitions[sel as usize].has_breakpoint;
                        refresh_listview(tm);
                    }
                }
                ID_BTN_ADD_STATE_BP => {
                    let state = get_window_text(tm.h_state_bp_edit);
                    let state = state.trim().to_string();
                    if !state.is_empty() {
                        if tm.state_breakpoints.contains(&state) {
                            tm.state_breakpoints.remove(&state);
                        } else {
                            tm.state_breakpoints.insert(state);
                        }
                        SetWindowTextW(tm.h_state_bp_edit, w!(""));
                        update_status(tm);
                    }
                }
                _ => {
                    // Handle ListView item click via notification
                    if notification == LBN_SELCHANGE as u32 {
                        // combo box change - ignore
                    }
                }
            }
            return LRESULT(0);
        }

        WM_NOTIFY => {
            if tm_ptr.is_null() {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let tm = &mut *tm_ptr;
            let nmhdr = *(lparam.0 as *const NMHDR);

            if nmhdr.hwndFrom == tm.h_listview {
                match nmhdr.code {
                    LVN_ITEMCHANGED => {
                        let sel = get_listview_selection(tm.h_listview);
                        if sel >= 0 && (sel as usize) < tm.transitions.len() {
                            populate_editor_from_transition(tm, sel as usize);
                        }
                    }
                    NM_CUSTOMDRAW => {
                        let cd = lparam.0 as *mut NMLVCUSTOMDRAW;
                        match (*cd).nmcd.dwDrawStage {
                            CDDS_PREPAINT => {
                                return LRESULT(CDRF_NOTIFYITEMDRAW as isize);
                            }
                            CDDS_ITEMPREPAINT => {
                                let item_idx = (*cd).nmcd.dwItemSpec;
                                if item_idx < tm.transitions.len() {
                                    if tm.transitions[item_idx].has_breakpoint {
                                        (*cd).clr_text = COLORREF(0x0000FF); // Red (BGR)
                                        if !tm.bold_font.is_invalid() {
                                            SelectObject((*cd).nmcd.hdc, tm.bold_font);
                                            return LRESULT(
                                                (CDRF_NOTIFYSUBITEMDRAW | CDRF_NEWFONT)
                                                    as isize,
                                            );
                                        }
                                    }
                                }
                                return LRESULT(CDRF_DODEFAULT as isize);
                            }
                            _ => {
                                return LRESULT(CDRF_DODEFAULT as isize);
                            }
                        }
                    }
                    _ => {}
                }
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        WM_TIMER => {
            if tm_ptr.is_null() {
                return LRESULT(0);
            }
            let tm = &mut *tm_ptr;
            if wparam.0 == ID_TIMER {
                let can_continue = tm.step();
                update_status(tm);
                InvalidateRect(hwnd, None, true);
                if !can_continue {
                    KillTimer(hwnd, ID_TIMER);
                    if tm.status == RunStatus::Running {
                        tm.status = RunStatus::Idle; // Paused by breakpoint
                    }
                    update_status(tm);
                }
            }
            return LRESULT(0);
        }

        WM_HSCROLL => {
            if tm_ptr.is_null() {
                return LRESULT(0);
            }
            let tm = &mut *tm_ptr;
            let ctrl = HWND(lparam.0 as isize);
            if ctrl == tm.h_speed_trackbar {
                const TBM_GETPOS_MSG: u32 = 0x0400; // WM_USER + 0
                let pos = SendMessageW(tm.h_speed_trackbar, TBM_GETPOS_MSG, WPARAM(0), LPARAM(0));
                tm.timer_speed_ms = pos.0 as u32;
                // If running, restart timer with new speed
                if tm.status == RunStatus::Running {
                    KillTimer(hwnd, ID_TIMER);
                    SetTimer(hwnd, ID_TIMER, tm.timer_speed_ms, None);
                }
            }
            return LRESULT(0);
        }

        WM_DESTROY => {
            if !tm_ptr.is_null() {
                let tm = &mut *tm_ptr;
                if !tm.ui_font.is_invalid() {
                    let _ = DeleteObject(tm.ui_font);
                }
                if !tm.bold_font.is_invalid() {
                    let _ = DeleteObject(tm.bold_font);
                }
                let _ = Box::from_raw(tm_ptr); // Free TM
            }
            PostQuitMessage(0);
            return LRESULT(0);
        }

        _ => {}
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// ── UI Helpers ──────────────────────────────────────────────────────────────

unsafe fn read_transition_from_editor(tm: &TuringMachine) -> Option<Transition> {
    let cur_state = get_window_text(tm.h_edit_cur_state);
    let new_state = get_window_text(tm.h_edit_new_state);
    if cur_state.trim().is_empty() || new_state.trim().is_empty() {
        return None;
    }

    let read_idx = SendMessageW(tm.h_combo_read, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    let write_idx = SendMessageW(tm.h_combo_write, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;
    let dir_idx = SendMessageW(tm.h_combo_dir, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0;

    if read_idx < 0 || write_idx < 0 || dir_idx < 0 {
        return None;
    }

    let symbols = [Symbol::Zero, Symbol::One, Symbol::Blank];
    let dirs = [Direction::Left, Direction::Right];

    Some(Transition {
        current_state: cur_state.trim().to_string(),
        read_symbol: symbols[read_idx as usize],
        new_state: new_state.trim().to_string(),
        write_symbol: symbols[write_idx as usize],
        direction: dirs[dir_idx as usize],
        has_breakpoint: false,
    })
}

unsafe fn populate_editor_from_transition(tm: &TuringMachine, idx: usize) {
    let t = &tm.transitions[idx];
    let cs = to_wide(&t.current_state);
    SetWindowTextW(tm.h_edit_cur_state, PCWSTR(cs.as_ptr()));
    let ns = to_wide(&t.new_state);
    SetWindowTextW(tm.h_edit_new_state, PCWSTR(ns.as_ptr()));
    SendMessageW(
        tm.h_combo_read,
        CB_SETCURSEL,
        WPARAM(t.read_symbol.index() as usize),
        LPARAM(0),
    );
    SendMessageW(
        tm.h_combo_write,
        CB_SETCURSEL,
        WPARAM(t.write_symbol.index() as usize),
        LPARAM(0),
    );
    SendMessageW(
        tm.h_combo_dir,
        CB_SETCURSEL,
        WPARAM(t.direction.index() as usize),
        LPARAM(0),
    );
}

unsafe fn get_listview_selection(hlv: HWND) -> i32 {
    SendMessageW(hlv, LVM_GETNEXTITEM, WPARAM(usize::MAX), LPARAM(LVNI_SELECTED as isize)).0
        as i32
}

unsafe fn refresh_listview(tm: &TuringMachine) {
    SendMessageW(tm.h_listview, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));

    for (i, t) in tm.transitions.iter().enumerate() {
        // Insert item (column 0)
        let cs = to_wide(&t.current_state);
        let mut lvi = LVITEMW {
            mask: LVIF_TEXT,
            iItem: i as i32,
            iSubItem: 0,
            pszText: PWSTR(cs.as_ptr() as *mut u16),
            ..Default::default()
        };
        SendMessageW(
            tm.h_listview,
            LVM_INSERTITEMW,
            WPARAM(0),
            LPARAM(&lvi as *const _ as isize),
        );

        // Read symbol (column 1)
        let rs = to_wide(t.read_symbol.display());
        lvi.iSubItem = 1;
        lvi.pszText = PWSTR(rs.as_ptr() as *mut u16);
        SendMessageW(
            tm.h_listview,
            LVM_SETITEMTEXTW,
            WPARAM(i),
            LPARAM(&lvi as *const _ as isize),
        );

        // New state (column 2)
        let ns = to_wide(&t.new_state);
        lvi.iSubItem = 2;
        lvi.pszText = PWSTR(ns.as_ptr() as *mut u16);
        SendMessageW(
            tm.h_listview,
            LVM_SETITEMTEXTW,
            WPARAM(i),
            LPARAM(&lvi as *const _ as isize),
        );

        // Write symbol (column 3)
        let ws = to_wide(t.write_symbol.display());
        lvi.iSubItem = 3;
        lvi.pszText = PWSTR(ws.as_ptr() as *mut u16);
        SendMessageW(
            tm.h_listview,
            LVM_SETITEMTEXTW,
            WPARAM(i),
            LPARAM(&lvi as *const _ as isize),
        );

        // Direction (column 4)
        let ds = to_wide(t.direction.display());
        lvi.iSubItem = 4;
        lvi.pszText = PWSTR(ds.as_ptr() as *mut u16);
        SendMessageW(
            tm.h_listview,
            LVM_SETITEMTEXTW,
            WPARAM(i),
            LPARAM(&lvi as *const _ as isize),
        );
    }
}

unsafe fn update_status(tm: &TuringMachine) {
    let bp_list: Vec<&String> = tm.state_breakpoints.iter().collect();
    let bp_str = if bp_list.is_empty() {
        String::new()
    } else {
        format!(
            "   State BPs: {}",
            bp_list
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let text = format!(
        "State: {}  |  Steps: {}  |  Status: {}{}",
        tm.current_state,
        tm.step_count,
        tm.status.display(),
        bp_str
    );
    let w = to_wide(&text);
    SetWindowTextW(tm.h_status_label, PCWSTR(w.as_ptr()));
}

// ── Create child controls ───────────────────────────────────────────────────

const WM_SETFONT: u32 = 0x0030;

unsafe fn send_font(hwnd: HWND, font: HFONT) {
    SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
}

unsafe fn create_controls(hwnd: HWND, hinst: HINSTANCE, tm: &mut TuringMachine) {
    let font = tm.ui_font;
    let lv_style = WINDOW_STYLE(
        WS_CHILD.0 | WS_VISIBLE.0 | WS_BORDER.0 | LVS_REPORT as u32 | LVS_SINGLESEL as u32
            | LVS_SHOWSELALWAYS as u32,
    );

    tm.h_listview = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("SysListView32"),
        w!(""),
        lv_style,
        10,
        85,
        960,
        265,
        hwnd,
        HMENU(ID_LISTVIEW as isize),
        hinst,
        None,
    );

    send_font(tm.h_listview, font);

    // Enable grid lines and full-row select
    SendMessageW(
        tm.h_listview,
        LVM_SETEXTENDEDLISTVIEWSTYLE,
        WPARAM(0),
        LPARAM((LVS_EX_FULLROWSELECT | LVS_EX_GRIDLINES) as isize),
    );

    // Add columns
    let col_headers = ["Current State", "Read", "New State", "Write", "Dir"];
    let col_widths = [180, 80, 180, 80, 80];

    for (i, (header, width)) in col_headers.iter().zip(col_widths.iter()).enumerate() {
        let w_header = to_wide(header);
        let col = LVCOLUMNW {
            mask: LVCF_TEXT | LVCF_WIDTH | LVCF_SUBITEM,
            cx: *width,
            pszText: PWSTR(w_header.as_ptr() as *mut u16),
            iSubItem: i as i32,
            ..Default::default()
        };
        SendMessageW(
            tm.h_listview,
            LVM_INSERTCOLUMNW,
            WPARAM(i),
            LPARAM(&col as *const _ as isize),
        );
    }

    // ── Transition Editor (y=355..430) ──
    let editor_y = 355;
    let label_h = 18;
    let ctrl_h = 24;

    // Labels
    create_static(hwnd, hinst, "Cur State:", 10, editor_y, 80, label_h, font);
    create_static(hwnd, hinst, "Read:", 170, editor_y, 50, label_h, font);
    create_static(hwnd, hinst, "New State:", 300, editor_y, 80, label_h, font);
    create_static(hwnd, hinst, "Write:", 460, editor_y, 50, label_h, font);
    create_static(hwnd, hinst, "Dir:", 590, editor_y, 40, label_h, font);

    let ctrl_y = editor_y + label_h + 2;

    // Edit: Current State
    tm.h_edit_cur_state = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        w!("EDIT"),
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        10,
        ctrl_y,
        150,
        ctrl_h,
        hwnd,
        HMENU(ID_EDIT_CUR_STATE as isize),
        hinst,
        None,
    );
    send_font(tm.h_edit_cur_state, font);

    // Combo: Read symbol
    tm.h_combo_read = create_combo(hwnd, hinst, 170, ctrl_y, 120, 100, ID_COMBO_READ, font);
    add_combo_items(tm.h_combo_read, &["0", "1", "_"]);

    // Edit: New State
    tm.h_edit_new_state = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        w!("EDIT"),
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        300,
        ctrl_y,
        150,
        ctrl_h,
        hwnd,
        HMENU(ID_EDIT_NEW_STATE as isize),
        hinst,
        None,
    );
    send_font(tm.h_edit_new_state, font);

    // Combo: Write symbol
    tm.h_combo_write = create_combo(hwnd, hinst, 460, ctrl_y, 120, 100, ID_COMBO_WRITE, font);
    add_combo_items(tm.h_combo_write, &["0", "1", "_"]);

    // Combo: Direction
    tm.h_combo_dir = create_combo(hwnd, hinst, 590, ctrl_y, 80, 100, ID_COMBO_DIR, font);
    add_combo_items(tm.h_combo_dir, &["L", "R"]);

    // Buttons: Add, Update, Delete
    let btn_y = ctrl_y + ctrl_h + 5;
    create_button(hwnd, hinst, "Add", 10, btn_y, 80, 28, ID_BTN_ADD, font);
    create_button(hwnd, hinst, "Update", 100, btn_y, 80, 28, ID_BTN_UPDATE, font);
    create_button(hwnd, hinst, "Delete", 190, btn_y, 80, 28, ID_BTN_DELETE, font);

    // ── Control Area (y=435..500) ──
    let ctrl_area_y = 440;

    create_button(hwnd, hinst, "Step", 10, ctrl_area_y, 70, 30, ID_BTN_STEP, font);
    create_button(hwnd, hinst, "Run", 90, ctrl_area_y, 70, 30, ID_BTN_RUN, font);
    create_button(hwnd, hinst, "Stop", 170, ctrl_area_y, 70, 30, ID_BTN_STOP, font);
    create_button(hwnd, hinst, "Reset", 250, ctrl_area_y, 70, 30, ID_BTN_RESET, font);
    create_button(
        hwnd,
        hinst,
        "Toggle BP",
        340,
        ctrl_area_y,
        90,
        30,
        ID_BTN_TOGGLE_BP,
        font,
    );

    // Speed label + trackbar
    create_static(hwnd, hinst, "Speed (ms):", 460, ctrl_area_y + 5, 80, 20, font);

    tm.h_speed_trackbar = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("msctls_trackbar32"),
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 0x0001 /* TBS_AUTOTICKS */),
        545,
        ctrl_area_y,
        200,
        30,
        hwnd,
        HMENU(ID_TRACKBAR as isize),
        hinst,
        None,
    );
    SendMessageW(
        tm.h_speed_trackbar,
        TBM_SETRANGE,
        WPARAM(1),
        LPARAM(((2000 << 16) | 50) as isize),
    );
    SendMessageW(
        tm.h_speed_trackbar,
        TBM_SETPOS,
        WPARAM(1),
        LPARAM(tm.timer_speed_ms as isize),
    );
    send_font(tm.h_speed_trackbar, font);

    // State breakpoint input
    create_static(hwnd, hinst, "State BP:", 760, ctrl_area_y + 5, 70, 20, font);
    tm.h_state_bp_edit = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        w!("EDIT"),
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        835,
        ctrl_area_y + 2,
        80,
        24,
        hwnd,
        HMENU(ID_STATE_BP_EDIT as isize),
        hinst,
        None,
    );
    send_font(tm.h_state_bp_edit, font);
    create_button(
        hwnd,
        hinst,
        "Add/Rm BP",
        920,
        ctrl_area_y,
        80,
        30,
        ID_BTN_ADD_STATE_BP,
        font,
    );

    // ── Status Bar (y=505..530) ──
    tm.h_status_label = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        w!("State: q0  |  Steps: 0  |  Status: Idle"),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0), // SS_LEFT = 0
        10,
        505,
        970,
        25,
        hwnd,
        HMENU(ID_STATUS_LABEL as isize),
        hinst,
        None,
    );
    send_font(tm.h_status_label, font);
}

unsafe fn create_static(
    parent: HWND,
    hinst: HINSTANCE,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    font: HFONT,
) -> HWND {
    let wt = to_wide(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("STATIC"),
        PCWSTR(wt.as_ptr()),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0),
        x,
        y,
        w,
        h,
        parent,
        None,
        hinst,
        None,
    );
    send_font(hwnd, font);
    hwnd
}

unsafe fn create_button(
    parent: HWND,
    hinst: HINSTANCE,
    text: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    font: HFONT,
) -> HWND {
    let wt = to_wide(text);
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        PCWSTR(wt.as_ptr()),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as isize),
        hinst,
        None,
    );
    send_font(hwnd, font);
    hwnd
}

unsafe fn create_combo(
    parent: HWND,
    hinst: HINSTANCE,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
    font: HFONT,
) -> HWND {
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("COMBOBOX"),
        w!(""),
        WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | (CBS_DROPDOWNLIST as u32)),
        x,
        y,
        w,
        h,
        parent,
        HMENU(id as isize),
        hinst,
        None,
    );
    send_font(hwnd, font);
    hwnd
}

unsafe fn add_combo_items(hcombo: HWND, items: &[&str]) {
    for item in items {
        let w = to_wide(item);
        SendMessageW(
            hcombo,
            CB_ADDSTRING,
            WPARAM(0),
            LPARAM(w.as_ptr() as isize),
        );
    }
    SendMessageW(hcombo, CB_SETCURSEL, WPARAM(0), LPARAM(0));
}

// ── Entry Point ─────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    unsafe {
        // Init common controls (ListView, Trackbar)
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_BAR_CLASSES,
        };
        InitCommonControlsEx(&icc);

        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("TuringMachineClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as isize),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Turing Machine Simulator"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            1020,
            580,
            None,
            None,
            hinstance,
            None,
        );

        // Create TuringMachine on heap
        let mut tm = Box::new(TuringMachine::new());

        // Create Segoe UI font for all controls and paint
        let mut face_name = [0u16; 32];
        let segoe = to_wide("Segoe UI");
        face_name[..segoe.len()].copy_from_slice(&segoe);
        let lf_ui = LOGFONTW {
            lfHeight: -14,
            lfWeight: FW_NORMAL.0 as i32,
            lfFaceName: face_name,
            ..Default::default()
        };
        tm.ui_font = CreateFontIndirectW(&lf_ui);

        // Create bold font for breakpoint rows
        let lf = LOGFONTW {
            lfWeight: FW_BOLD.0 as i32,
            lfItalic: 1,
            lfHeight: -14,
            lfFaceName: face_name,
            ..Default::default()
        };
        tm.bold_font = CreateFontIndirectW(&lf);

        // Create all child controls
        create_controls(hwnd, hinstance.into(), &mut tm);

        // Store pointer in window user data
        let raw = Box::into_raw(tm);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}
