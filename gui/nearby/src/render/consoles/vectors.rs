use super::Platform;
use mirui::prelude::*;
use mirui::render::path::Path;

pub(super) struct Part {
    pub color: Color,
    pub path: Path,
}

macro_rules! part {
    ($r:literal, $g:literal, $b:literal; $($shape:tt)+) => {
        Part { color: Color::rgb($r, $g, $b), path: path!($($shape)+) }
    };
}

pub(super) fn machine(platform: Platform) -> &'static [Part] {
    match platform {
        Platform::Favorites => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 27 15 H 103 V 94 H 27 Z),
                part!(77, 101, 78; M 37 23 H 113 V 102 H 37 Z),
                part!(176, 184, 155; M 47 31 H 123 V 110 H 47 Z),
                part!(41, 66, 52; M 53 38 H 117 V 84 H 53 Z),
                part!(204, 148, 66; M 61 45 H 109 V 77 H 61 Z),
                part!(58, 83, 65; M 58 91 H 113 V 101 H 58 Z),
                part!(212, 181, 110; M 62 95 H 109 V 101 H 62 Z),
                part!(239, 232, 201; M 85 48 V 55 H 78 V 62 H 71 V 69 H 80 V 78 H 91 V 69 H 100 V 62 H 93 V 55 H 86 V 48 Z),
            ];
            PARTS
        }
        Platform::Nes => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 19 30 H 142 V 89 H 19 Z),
                part!(217, 217, 189; M 21 27 H 140 V 78 H 21 Z),
                part!(165, 76, 59; M 21 68 H 140 V 82 H 21 Z),
                part!(179, 179, 154; M 42 27 H 119 V 63 H 42 Z),
                part!(41, 60, 51; M 47 32 H 114 V 57 H 47 Z),
                part!(165, 76, 59; M 24 49 H 36 V 59 H 24 Z),
                part!(165, 76, 59; M 122 49 H 134 V 59 H 122 Z),
                part!(41, 60, 51; M 44 86 H 69 V 91 H 44 Z),
                part!(41, 60, 51; M 101 86 H 126 V 91 H 101 Z),
            ];
            PARTS
        }
        Platform::Fds => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 20 38 H 140 V 85 H 20 Z),
                part!(165, 76, 59; M 23 35 H 137 V 81 H 23 Z),
                part!(217, 217, 189; M 28 40 H 132 V 53 H 28 Z),
                part!(41, 60, 51; M 39 59 H 119 V 73 H 39 Z),
                part!(168, 175, 152; M 44 61 H 114 V 64 H 44 Z),
                part!(210, 165, 79; M 119 65 H 128 V 74 H 119 Z),
                part!(168, 175, 152; M 63 19 H 98 V 46 H 63 Z),
                part!(41, 60, 51; M 68 22 H 93 V 35 H 68 Z),
                part!(217, 217, 189; M 70 37 H 90 V 44 H 70 Z),
            ];
            PARTS
        }
        Platform::Snes => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 20 33 H 140 V 87 H 20 Z),
                part!(217, 217, 189; M 23 27 H 137 V 84 H 23 Z),
                part!(168, 175, 152; M 35 28 H 126 V 50 H 35 Z),
                part!(41, 60, 51; M 45 32 H 117 V 40 H 45 Z),
                part!(139, 119, 161; M 44 57 H 62 V 72 H 44 Z),
                part!(139, 119, 161; M 99 57 H 117 V 72 H 99 Z),
                part!(41, 60, 51; M 66 65 H 93 V 68 H 66 Z),
                part!(41, 60, 51; M 35 79 H 62 V 85 H 35 Z),
                part!(41, 60, 51; M 100 79 H 127 V 85 H 100 Z),
            ];
            PARTS
        }
        Platform::Megadrive => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 20 34 H 139 V 81 H 20 Z),
                part!(65, 79, 69; M 24 30 H 135 V 74 H 24 Z),
                part!(41, 60, 51; M 105 51 L 103 61 L 98 69 L 90 74 L 80 76 L 70 74 L 62 69 L 57 61 L 55 51 L 57 41 L 62 33 L 70 28 L 80 26 L 90 28 L 98 33 L 103 41 Z),
                part!(168, 175, 152; M 48 41 H 115 V 49 H 48 Z),
                part!(165, 76, 59; M 27 59 H 40 V 65 H 27 Z),
                part!(41, 60, 51; M 35 72 H 62 V 79 H 35 Z),
                part!(41, 60, 51; M 97 72 H 124 V 79 H 97 Z),
            ];
            PARTS
        }
        Platform::Mastersystem => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 17 37 H 143 V 85 H 17 Z),
                part!(66, 82, 71; M 20 34 H 140 V 75 H 20 Z),
                part!(165, 76, 59; M 24 39 H 136 V 47 H 24 Z),
                part!(41, 60, 51; M 51 48 H 113 V 56 H 51 Z),
                part!(165, 76, 59; M 27 64 H 50 V 70 H 27 Z),
                part!(41, 60, 51; M 67 74 H 96 V 82 H 67 Z),
                part!(41, 60, 51; M 106 74 H 135 V 82 H 106 Z),
            ];
            PARTS
        }
        Platform::Segacd => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 12 38 H 151 V 98 H 12 Z),
                part!(86, 98, 86; M 16 41 H 147 V 94 H 16 Z),
                part!(41, 60, 51; M 18 83 H 99 V 88 H 18 Z),
                part!(210, 165, 79; M 121 84 H 139 V 88 H 121 Z),
                part!(41, 60, 51; M 16 34 H 135 V 81 H 16 Z),
                part!(65, 79, 69; M 20 30 H 131 V 74 H 20 Z),
                part!(41, 60, 51; M 105 51 L 103 61 L 98 69 L 90 74 L 80 76 L 70 74 L 62 69 L 57 61 L 55 51 L 57 41 L 62 33 L 70 28 L 80 26 L 90 28 L 98 33 L 103 41 Z),
                part!(168, 175, 152; M 48 41 H 115 V 49 H 48 Z),
                part!(165, 76, 59; M 27 59 H 40 V 65 H 27 Z),
                part!(41, 60, 51; M 35 72 H 62 V 79 H 35 Z),
                part!(41, 60, 51; M 97 72 H 124 V 79 H 97 Z),
            ];
            PARTS
        }
        Platform::Sega32X => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 20 34 H 139 V 81 H 20 Z),
                part!(65, 79, 69; M 24 30 H 135 V 74 H 24 Z),
                part!(41, 60, 51; M 105 51 L 103 61 L 98 69 L 90 74 L 80 76 L 70 74 L 62 69 L 57 61 L 55 51 L 57 41 L 62 33 L 70 28 L 80 26 L 90 28 L 98 33 L 103 41 Z),
                part!(168, 175, 152; M 48 41 H 115 V 49 H 48 Z),
                part!(165, 76, 59; M 27 59 H 40 V 65 H 27 Z),
                part!(41, 60, 51; M 35 72 H 62 V 79 H 35 Z),
                part!(41, 60, 51; M 97 72 H 124 V 79 H 97 Z),
                part!(41, 60, 51; M 49 17 H 110 V 45 H 49 Z),
                part!(89, 103, 90; M 54 13 H 105 V 41 H 54 Z),
                part!(41, 60, 51; M 62 16 H 97 V 23 H 62 Z),
                part!(210, 165, 79; M 58 33 H 102 V 37 H 58 Z),
            ];
            PARTS
        }
        Platform::Pcengine => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 39 26 H 121 V 92 H 39 Z),
                part!(217, 217, 189; M 41 24 H 119 V 86 H 41 Z),
                part!(168, 175, 152; M 48 28 H 112 V 61 H 48 Z),
                part!(41, 60, 51; M 54 32 H 106 V 60 H 54 Z),
                part!(165, 76, 59; M 58 34 H 102 V 56 H 58 Z),
                part!(41, 60, 51; M 49 68 H 93 V 73 H 49 Z),
                part!(164, 118, 73; M 101 67 H 112 V 76 H 101 Z),
                part!(41, 60, 51; M 68 82 H 92 V 88 H 68 Z),
            ];
            PARTS
        }
        Platform::Arcade => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 42 8 H 117 V 26 H 42 Z),
                part!(165, 76, 59; M 46 12 H 113 V 21 H 46 Z),
                part!(41, 60, 51; M 47 26 H 110 V 77 H 47 Z),
                part!(168, 175, 152; M 54 30 H 102 V 62 H 54 Z),
                part!(88, 118, 107; M 58 34 H 98 V 58 H 58 Z),
                part!(165, 76, 59; M 47 65 H 110 L 123 80 V 89 H 34 V 80 Z),
                part!(41, 60, 51; M 40 89 H 117 V 114 H 40 Z),
                part!(82, 105, 88; M 47 91 H 110 V 110 H 47 Z),
                part!(41, 60, 51; M 53 72 H 56 V 81 H 53 Z),
                part!(210, 165, 79; M 50 69 H 59 V 74 H 50 Z),
                part!(210, 165, 79; M 88 75 H 95 V 79 H 88 Z),
                part!(217, 217, 189; M 101 75 H 108 V 79 H 101 Z),
                part!(41, 60, 51; M 64 97 H 91 V 101 H 64 Z),
            ];
            PARTS
        }
        Platform::Neogeo => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 15 38 H 147 V 87 H 15 Z),
                part!(71, 86, 74; M 20 32 H 142 V 80 H 20 Z),
                part!(41, 60, 51; M 42 30 H 120 V 50 H 42 Z),
                part!(210, 165, 79; M 50 34 H 114 V 41 H 50 Z),
                part!(41, 60, 51; M 23 63 H 40 V 70 H 23 Z),
                part!(41, 60, 51; M 113 64 H 132 V 71 H 113 Z),
                part!(210, 165, 79; M 56 64 H 99 V 68 H 56 Z),
            ];
            PARTS
        }
        Platform::Cps1 => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 16 28 H 146 V 92 H 16 Z),
                part!(82, 128, 91; M 19 25 H 143 V 86 H 19 Z),
                part!(210, 165, 79; M 23 82 H 139 V 89 H 23 Z),
                part!(41, 60, 51; M 26 32 H 45 V 42 H 26 Z),
                part!(168, 175, 152; M 29 34 H 41 V 36 H 29 Z),
                part!(41, 60, 51; M 53 32 H 72 V 42 H 53 Z),
                part!(168, 175, 152; M 56 34 H 68 V 36 H 56 Z),
                part!(41, 60, 51; M 80 32 H 99 V 42 H 80 Z),
                part!(168, 175, 152; M 83 34 H 95 V 36 H 83 Z),
                part!(41, 60, 51; M 107 32 H 126 V 42 H 107 Z),
                part!(168, 175, 152; M 110 34 H 122 V 36 H 110 Z),
                part!(41, 60, 51; M 26 48 H 45 V 58 H 26 Z),
                part!(168, 175, 152; M 29 50 H 41 V 52 H 29 Z),
                part!(41, 60, 51; M 53 48 H 72 V 58 H 53 Z),
                part!(168, 175, 152; M 56 50 H 68 V 52 H 56 Z),
                part!(41, 60, 51; M 80 48 H 99 V 58 H 80 Z),
                part!(168, 175, 152; M 83 50 H 95 V 52 H 83 Z),
                part!(41, 60, 51; M 107 48 H 126 V 58 H 107 Z),
                part!(168, 175, 152; M 110 50 H 122 V 52 H 110 Z),
                part!(41, 60, 51; M 26 64 H 45 V 74 H 26 Z),
                part!(168, 175, 152; M 29 66 H 41 V 68 H 29 Z),
                part!(41, 60, 51; M 53 64 H 72 V 74 H 53 Z),
                part!(168, 175, 152; M 56 66 H 68 V 68 H 56 Z),
                part!(41, 60, 51; M 80 64 H 99 V 74 H 80 Z),
                part!(168, 175, 152; M 83 66 H 95 V 68 H 83 Z),
                part!(41, 60, 51; M 107 64 H 126 V 74 H 107 Z),
                part!(168, 175, 152; M 110 66 H 122 V 68 H 110 Z),
                part!(217, 217, 189; M 108 53 H 137 V 77 H 108 Z),
                part!(165, 76, 59; M 112 57 H 133 V 73 H 112 Z),
                part!(217, 217, 189; M 114 60 H 117 V 68 H 114 Z),
            ];
            PARTS
        }
        Platform::Cps2 => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 16 28 H 146 V 92 H 16 Z),
                part!(100, 125, 154; M 19 25 H 143 V 86 H 19 Z),
                part!(210, 165, 79; M 23 82 H 139 V 89 H 23 Z),
                part!(41, 60, 51; M 26 32 H 45 V 42 H 26 Z),
                part!(168, 175, 152; M 29 34 H 41 V 36 H 29 Z),
                part!(41, 60, 51; M 53 32 H 72 V 42 H 53 Z),
                part!(168, 175, 152; M 56 34 H 68 V 36 H 56 Z),
                part!(41, 60, 51; M 80 32 H 99 V 42 H 80 Z),
                part!(168, 175, 152; M 83 34 H 95 V 36 H 83 Z),
                part!(41, 60, 51; M 107 32 H 126 V 42 H 107 Z),
                part!(168, 175, 152; M 110 34 H 122 V 36 H 110 Z),
                part!(41, 60, 51; M 26 48 H 45 V 58 H 26 Z),
                part!(168, 175, 152; M 29 50 H 41 V 52 H 29 Z),
                part!(41, 60, 51; M 53 48 H 72 V 58 H 53 Z),
                part!(168, 175, 152; M 56 50 H 68 V 52 H 56 Z),
                part!(41, 60, 51; M 80 48 H 99 V 58 H 80 Z),
                part!(168, 175, 152; M 83 50 H 95 V 52 H 83 Z),
                part!(41, 60, 51; M 107 48 H 126 V 58 H 107 Z),
                part!(168, 175, 152; M 110 50 H 122 V 52 H 110 Z),
                part!(41, 60, 51; M 26 64 H 45 V 74 H 26 Z),
                part!(168, 175, 152; M 29 66 H 41 V 68 H 29 Z),
                part!(41, 60, 51; M 53 64 H 72 V 74 H 53 Z),
                part!(168, 175, 152; M 56 66 H 68 V 68 H 56 Z),
                part!(41, 60, 51; M 80 64 H 99 V 74 H 80 Z),
                part!(168, 175, 152; M 83 66 H 95 V 68 H 83 Z),
                part!(41, 60, 51; M 107 64 H 126 V 74 H 107 Z),
                part!(168, 175, 152; M 110 66 H 122 V 68 H 110 Z),
                part!(217, 217, 189; M 108 53 H 137 V 77 H 108 Z),
                part!(165, 76, 59; M 112 57 H 133 V 73 H 112 Z),
                part!(217, 217, 189; M 114 60 H 117 V 68 H 114 Z M 120 60 H 123 V 68 H 120 Z),
            ];
            PARTS
        }
        Platform::Cps3 => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 16 28 H 146 V 92 H 16 Z),
                part!(108, 152, 123; M 19 25 H 143 V 86 H 19 Z),
                part!(210, 165, 79; M 23 82 H 139 V 89 H 23 Z),
                part!(41, 60, 51; M 26 32 H 45 V 42 H 26 Z),
                part!(168, 175, 152; M 29 34 H 41 V 36 H 29 Z),
                part!(41, 60, 51; M 53 32 H 72 V 42 H 53 Z),
                part!(168, 175, 152; M 56 34 H 68 V 36 H 56 Z),
                part!(41, 60, 51; M 80 32 H 99 V 42 H 80 Z),
                part!(168, 175, 152; M 83 34 H 95 V 36 H 83 Z),
                part!(41, 60, 51; M 107 32 H 126 V 42 H 107 Z),
                part!(168, 175, 152; M 110 34 H 122 V 36 H 110 Z),
                part!(41, 60, 51; M 26 48 H 45 V 58 H 26 Z),
                part!(168, 175, 152; M 29 50 H 41 V 52 H 29 Z),
                part!(41, 60, 51; M 53 48 H 72 V 58 H 53 Z),
                part!(168, 175, 152; M 56 50 H 68 V 52 H 56 Z),
                part!(41, 60, 51; M 80 48 H 99 V 58 H 80 Z),
                part!(168, 175, 152; M 83 50 H 95 V 52 H 83 Z),
                part!(41, 60, 51; M 107 48 H 126 V 58 H 107 Z),
                part!(168, 175, 152; M 110 50 H 122 V 52 H 110 Z),
                part!(41, 60, 51; M 26 64 H 45 V 74 H 26 Z),
                part!(168, 175, 152; M 29 66 H 41 V 68 H 29 Z),
                part!(41, 60, 51; M 53 64 H 72 V 74 H 53 Z),
                part!(168, 175, 152; M 56 66 H 68 V 68 H 56 Z),
                part!(41, 60, 51; M 80 64 H 99 V 74 H 80 Z),
                part!(168, 175, 152; M 83 66 H 95 V 68 H 83 Z),
                part!(41, 60, 51; M 107 64 H 126 V 74 H 107 Z),
                part!(168, 175, 152; M 110 66 H 122 V 68 H 110 Z),
                part!(217, 217, 189; M 108 53 H 137 V 77 H 108 Z),
                part!(165, 76, 59; M 112 57 H 133 V 73 H 112 Z),
                part!(217, 217, 189; M 114 60 H 117 V 68 H 114 Z M 120 60 H 123 V 68 H 120 Z M 126 60 H 129 V 68 H 126 Z),
            ];
            PARTS
        }
        Platform::Psx => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 18 32 H 142 V 85 H 18 Z),
                part!(217, 217, 189; M 20 29 H 140 V 81 H 20 Z),
                part!(168, 175, 152; M 39 36 H 121 V 74 H 39 Z),
                part!(217, 217, 189; M 102 55 L 100 63 L 96 71 L 88 75 L 80 77 L 72 75 L 64 71 L 60 63 L 58 55 L 60 47 L 64 39 L 72 35 L 80 33 L 88 35 L 96 39 L 100 47 Z),
                part!(41, 60, 51; M 83 55 L 83 56 L 82 57 L 81 58 L 80 58 L 79 58 L 78 57 L 77 56 L 77 55 L 77 54 L 78 53 L 79 52 L 80 52 L 81 52 L 82 53 L 83 54 Z),
                part!(132, 167, 111; M 25 40 H 34 V 48 H 25 Z),
                part!(168, 175, 152; M 126 40 H 134 V 48 H 126 Z),
                part!(41, 60, 51; M 37 79 H 64 V 84 H 37 Z),
                part!(41, 60, 51; M 95 79 H 122 V 84 H 95 Z),
            ];
            PARTS
        }
        Platform::Gb => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 47 7 H 112 V 105 H 47 Z),
                part!(202, 205, 184; M 50 7 H 109 V 101 H 50 Z),
                part!(168, 175, 152; M 54 17 H 105 V 60 H 54 Z),
                part!(41, 60, 51; M 58 21 H 101 V 54 H 58 Z),
                part!(166, 183, 118; M 61 24 H 98 V 51 H 61 Z),
                part!(41, 60, 51; M 61 69 H 66 V 84 H 61 Z),
                part!(41, 60, 51; M 56 74 H 71 V 79 H 56 Z),
                part!(165, 76, 59; M 88 72 H 95 V 79 H 88 Z),
                part!(165, 76, 59; M 98 65 H 105 V 72 H 98 Z),
                part!(41, 60, 51; M 73 88 H 81 V 91 H 73 Z),
                part!(41, 60, 51; M 85 88 H 93 V 91 H 85 Z),
                part!(41, 60, 51; M 92 97 H 106 V 99 H 92 Z),
            ];
            PARTS
        }
        Platform::Gbc => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 47 7 H 112 V 105 H 47 Z),
                part!(138, 101, 152; M 50 7 H 109 V 101 H 50 Z),
                part!(168, 175, 152; M 54 17 H 105 V 60 H 54 Z),
                part!(41, 60, 51; M 58 21 H 101 V 54 H 58 Z),
                part!(166, 183, 118; M 61 24 H 98 V 51 H 61 Z),
                part!(41, 60, 51; M 61 69 H 66 V 84 H 61 Z),
                part!(41, 60, 51; M 56 74 H 71 V 79 H 56 Z),
                part!(165, 76, 59; M 88 72 H 95 V 79 H 88 Z),
                part!(165, 76, 59; M 98 65 H 105 V 72 H 98 Z),
                part!(41, 60, 51; M 73 88 H 81 V 91 H 73 Z),
                part!(41, 60, 51; M 85 88 H 93 V 91 H 85 Z),
                part!(41, 60, 51; M 92 97 H 106 V 99 H 92 Z),
            ];
            PARTS
        }
        Platform::Gba => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 12 35 H 148 V 92 H 12 Z),
                part!(41, 60, 51; M 8 42 H 152 V 84 H 8 Z),
                part!(116, 100, 151; M 12 39 H 148 V 84 H 12 Z),
                part!(172, 164, 182; M 39 39 H 121 V 83 H 39 Z),
                part!(41, 60, 51; M 44 43 H 116 V 78 H 44 Z),
                part!(177, 185, 135; M 48 46 H 112 V 75 H 48 Z),
                part!(41, 60, 51; M 22 51 H 27 V 66 H 22 Z),
                part!(41, 60, 51; M 17 56 H 32 V 61 H 17 Z),
                part!(165, 76, 59; M 124 56 H 131 V 63 H 124 Z),
                part!(165, 76, 59; M 135 49 H 142 V 56 H 135 Z),
                part!(41, 60, 51; M 22 76 H 30 V 78 H 22 Z),
            ];
            PARTS
        }
        Platform::Psp => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 14 41 H 146 V 46 H 152 V 93 H 146 V 99 H 14 V 93 H 8 V 46 H 14 Z),
                part!(137, 146, 120; M 43 45 H 117 V 95 H 43 Z),
                part!(177, 185, 135; M 48 50 H 112 V 90 H 48 Z),
                part!(137, 146, 120; M 21 66 H 27 V 59 H 33 V 66 H 40 V 72 H 33 V 79 H 27 V 72 H 21 Z),
                part!(165, 76, 59; M 124 65 H 132 V 73 H 124 Z M 135 56 H 143 V 64 H 135 Z),
                part!(217, 217, 189; M 22 85 H 28 V 91 H 22 Z M 54 99 H 106 V 102 H 54 Z),
            ];
            PARTS
        }
        Platform::Gamegear => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 16 36 H 144 V 42 H 150 V 103 H 144 V 109 H 16 V 103 H 10 V 42 H 16 Z),
                part!(137, 146, 120; M 45 45 H 114 V 96 H 45 Z),
                part!(177, 185, 135; M 50 50 H 109 V 90 H 50 Z),
                part!(137, 146, 120; M 21 66 H 27 V 59 H 33 V 66 H 40 V 72 H 33 V 79 H 27 V 72 H 21 Z),
                part!(165, 76, 59; M 124 65 H 132 V 73 H 124 Z M 135 56 H 143 V 64 H 135 Z),
                part!(217, 217, 189; M 47 100 H 103 V 103 H 47 Z M 120 88 H 140 V 91 H 120 Z M 120 95 H 140 V 98 H 120 Z),
            ];
            PARTS
        }
        Platform::Lynx => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 5 42 H 155 V 106 H 5 Z),
                part!(137, 146, 120; M 44 47 H 116 V 100 H 44 Z),
                part!(177, 185, 135; M 49 53 H 111 V 93 H 49 Z),
                part!(137, 146, 120; M 21 66 H 27 V 59 H 33 V 66 H 40 V 72 H 33 V 79 H 27 V 72 H 21 Z),
                part!(165, 76, 59; M 124 65 H 132 V 73 H 124 Z M 135 56 H 143 V 64 H 135 Z),
                part!(204, 148, 66; M 12 86 H 36 V 89 H 12 Z M 12 93 H 36 V 96 H 12 Z M 122 83 H 147 V 87 H 122 Z),
            ];
            PARTS
        }
        Platform::Wonderswan => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 13 38 H 147 V 105 H 13 Z),
                part!(41, 60, 51; M 49 43 H 118 V 97 H 49 Z),
                part!(177, 185, 135; M 54 49 H 113 V 91 H 54 Z),
                part!(41, 60, 51; M 25 48 H 30 V 63 H 25 Z M 20 53 H 35 V 58 H 20 Z M 25 76 H 30 V 91 H 25 Z M 20 81 H 35 V 86 H 20 Z M 126 73 H 132 V 79 H 126 Z M 135 64 H 141 V 70 H 135 Z),
            ];
            PARTS
        }
        Platform::Ngp => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 25 26 H 135 V 114 H 25 Z),
                part!(41, 60, 51; M 49 34 H 113 V 95 H 49 Z),
                part!(177, 185, 135; M 55 40 H 107 V 84 H 55 Z),
                part!(41, 60, 51; M 32 62 H 44 V 77 H 32 Z M 119 55 H 128 V 64 H 119 Z M 119 79 H 128 V 88 H 119 Z),
                part!(204, 148, 66; M 65 100 H 76 V 104 H 65 Z M 84 100 H 95 V 104 H 84 Z),
            ];
            PARTS
        }
        Platform::Nds => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 30 12 H 130 V 70 H 30 Z M 26 75 H 134 V 138 H 26 Z),
                part!(41, 60, 51; M 48 19 H 112 V 63 H 48 Z M 47 82 H 113 V 129 H 47 Z),
                part!(177, 185, 135; M 54 25 H 106 V 57 H 54 Z M 53 88 H 107 V 123 H 53 Z),
                part!(41, 60, 51; M 33 94 H 39 V 111 H 33 Z M 28 100 H 44 V 105 H 28 Z M 119 94 H 125 V 100 H 119 Z M 126 102 H 132 V 108 H 126 Z M 116 109 H 122 V 115 H 116 Z),
            ];
            PARTS
        }
        Platform::Dreamcast => {
            static PARTS: &[Part] = &[
                part!(217, 217, 189; M 29 31 H 131 L 143 98 H 17 Z),
                part!(137, 146, 120; M 17 98 H 143 V 114 H 17 Z M 28 103 H 50 V 110 H 28 Z M 59 103 H 81 V 110 H 59 Z M 90 103 H 112 V 110 H 90 Z),
                part!(137, 146, 120; M 51 39 H 108 L 119 72 V 81 H 42 V 72 Z),
                part!(165, 76, 59; M 69 48 H 94 V 66 H 77 V 57 H 87 V 61 H 82 V 62 H 90 V 52 H 73 V 72 H 98 V 76 H 69 Z),
            ];
            PARTS
        }
        Platform::N64 => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 24 43 H 136 L 147 96 H 13 Z),
                part!(137, 146, 120; M 38 38 H 122 V 52 H 38 Z M 50 58 H 110 V 65 H 50 Z),
                part!(41, 60, 51; M 21 91 H 139 V 108 H 21 Z),
                part!(217, 217, 189; M 32 95 H 44 V 103 H 32 Z M 59 95 H 71 V 103 H 59 Z M 86 95 H 98 V 103 H 86 Z M 113 95 H 125 V 103 H 113 Z),
                part!(165, 76, 59; M 117 70 H 126 V 79 H 117 Z),
            ];
            PARTS
        }
        Platform::Saturn => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 26 32 H 134 L 145 96 H 15 Z M 15 96 H 145 V 111 H 15 Z),
                part!(137, 146, 120; M 53 41 H 108 L 117 55 V 82 H 43 V 55 Z),
                part!(41, 60, 51; M 61 49 H 100 L 107 59 V 76 H 53 V 59 Z),
                part!(217, 217, 189; M 23 88 H 37 V 94 H 23 Z M 121 88 H 135 V 94 H 121 Z M 35 102 H 61 V 108 H 35 Z M 100 102 H 126 V 108 H 100 Z),
            ];
            PARTS
        }
        Platform::Virtualboy => {
            static PARTS: &[Part] = &[
                part!(165, 76, 59; M 27 29 H 133 L 143 44 V 72 H 17 V 44 Z),
                part!(41, 60, 51; M 33 45 H 127 V 66 H 33 Z),
                part!(165, 76, 59; M 45 48 H 69 V 60 H 45 Z M 91 48 H 115 V 60 H 91 Z),
                part!(41, 60, 51; M 74 72 H 86 V 99 H 96 L 119 134 H 109 L 80 105 L 51 134 H 41 L 64 99 H 74 Z),
            ];
            PARTS
        }
        Platform::Computer => {
            static PARTS: &[Part] = &[
                part!(217, 217, 189; M 34 14 H 126 V 86 H 34 Z M 63 86 H 97 V 99 H 63 Z M 19 104 H 141 L 150 133 H 10 Z),
                part!(41, 60, 51; M 41 21 H 119 V 76 H 41 Z),
                part!(177, 185, 135; M 47 27 H 113 V 70 H 47 Z),
                part!(41, 60, 51; M 23 111 H 137 V 115 H 23 Z M 19 120 H 141 V 124 H 19 Z M 44 127 H 113 V 130 H 44 Z M 54 36 H 61 V 39 H 54 Z M 58 40 H 65 V 43 H 58 Z M 54 44 H 61 V 47 H 54 Z M 70 47 H 89 V 50 H 70 Z),
            ];
            PARTS
        }
        Platform::Phone => {
            static PARTS: &[Part] = &[
                part!(41, 60, 51; M 52 12 H 108 V 139 H 52 Z),
                part!(217, 217, 189; M 57 29 H 103 V 84 H 57 Z),
                part!(177, 185, 135; M 61 33 H 99 V 80 H 61 Z),
                part!(137, 146, 120; M 71 91 H 89 V 104 H 71 Z M 59 111 H 70 V 115 H 59 Z M 75 111 H 86 V 115 H 75 Z M 91 111 H 102 V 115 H 91 Z M 59 122 H 70 V 126 H 59 Z M 75 122 H 86 V 126 H 75 Z M 91 122 H 102 V 126 H 91 Z),
            ];
            PARTS
        }
        Platform::Engine => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 27 24 H 133 V 110 H 27 Z),
                part!(41, 60, 51; M 36 33 H 124 V 99 H 36 Z),
                part!(204, 148, 66; M 47 50 H 57 V 43 H 67 V 53 H 60 V 63 H 53 V 72 H 67 V 82 H 57 V 75 H 47 Z M 113 50 H 103 V 43 H 93 V 53 H 100 V 63 H 107 V 72 H 93 V 82 H 103 V 75 H 113 Z M 79 48 H 87 L 78 82 H 70 Z),
                part!(41, 60, 51; M 39 115 H 121 V 124 H 39 Z),
            ];
            PARTS
        }
        Platform::Ports => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 19 44 H 141 V 98 H 19 Z M 27 36 H 133 V 44 H 27 Z M 27 98 H 133 V 106 H 27 Z),
                part!(41, 60, 51; M 35 58 H 44 V 81 H 35 Z M 28 65 H 51 V 74 H 28 Z M 109 60 H 119 V 70 H 109 Z M 125 75 H 135 V 85 H 125 Z),
                part!(165, 76, 59; M 65 66 H 95 V 73 H 65 Z M 85 56 H 92 V 83 H 85 Z),
            ];
            PARTS
        }
        Platform::Unknown => {
            static PARTS: &[Part] = &[
                part!(137, 146, 120; M 19 44 H 141 V 98 H 19 Z M 27 36 H 133 V 44 H 27 Z M 27 98 H 133 V 106 H 27 Z),
                part!(41, 60, 51; M 35 58 H 44 V 81 H 35 Z M 28 65 H 51 V 74 H 28 Z M 109 60 H 119 V 70 H 109 Z M 125 75 H 135 V 85 H 125 Z),
                part!(165, 76, 59; M 65 66 H 95 V 73 H 65 Z M 85 56 H 92 V 83 H 85 Z),
            ];
            PARTS
        }
    }
}
