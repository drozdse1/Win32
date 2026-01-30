use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_COMMAND => {
                let cmd = wparam.0 & 0xffff;
                if cmd == 1 {
                    MessageBoxW(hwnd, w!("Hello from menu!"), w!("Info"), MB_OK);
                }
                LRESULT(0)
            }
            WM_CTLCOLORSTATIC => {
                let hdc = HDC(wparam.0 as isize);
                SetBkMode(hdc, TRANSPARENT);
                return LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() -> Result<()> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("MyWindowClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize),
            ..Default::default()
        };
        RegisterClassW(&wc);

        // Menü erstellen
        let hmenu = CreateMenu()?;
        AppendMenuW(hmenu, MF_STRING, 1, w!("Show Dialog"))?;

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Rust Win32 Window"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            500,
            400,
            None,
            hmenu,
            hinstance,
            None,
        );

        // Grid layout constants
        let grid_x = 20;
        let grid_y = 20;
        let col_width = 80;
        let row_height = 30;
        let headers = [w!("A"), w!("B"), w!("C"), w!("D")];

        // Create column headers
        for (col, header) in headers.iter().enumerate() {
            CreateWindowExW(
                Default::default(),
                w!("STATIC"),
                *header,
                WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | 1u32), // SS_CENTER = 1
                grid_x + col as i32 * col_width,
                grid_y,
                col_width,
                row_height,
                hwnd,
                None,
                hinstance,
                None,
            );
        }

        // Create 4x4 checkbox grid
        for row in 0..4 {
            for col in 0..4 {
                let id = (row * 4 + col + 100) as usize;
                CreateWindowExW(
                    Default::default(),
                    w!("BUTTON"),
                    w!(""),
                    WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_AUTOCHECKBOX as u32),
                    grid_x + col * col_width + col_width / 2 - 10,
                    grid_y + (row + 1) * row_height,
                    20,
                    20,
                    hwnd,
                    HMENU(id as isize),
                    hinstance,
                    None,
                );
            }
        }

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    }
}
