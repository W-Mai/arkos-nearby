#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct StateSlots {
    pub configured: i32,
    pub entry: Option<u16>,
    pub auto_index: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupSlot {
    Fixed(i32),
    AutoIndexed,
}

impl StateSlots {
    pub(super) fn resolve(configured: i32, argv: &[String], auto_index: bool) -> Self {
        let mut args = argv.iter();
        let mut entry = None;
        while let Some(arg) = args.next() {
            if arg == "--" {
                break;
            }
            let value = if arg == "-e" || arg == "--entryslot" {
                args.next().map(String::as_str)
            } else {
                arg.strip_prefix("--entryslot=")
                    .or_else(|| arg.strip_prefix("-e").filter(|s| !s.is_empty()))
            };
            if let Some(value) = value.and_then(entry_index) {
                entry = Some(value);
            }
        }
        Self {
            configured,
            entry,
            auto_index,
        }
    }

    pub fn startup(&self) -> StartupSlot {
        if let Some(entry) = self.entry {
            StartupSlot::Fixed(i32::from(entry))
        } else if self.auto_index {
            StartupSlot::AutoIndexed
        } else {
            StartupSlot::Fixed(0)
        }
    }
}

fn entry_index(text: &str) -> Option<u16> {
    let text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let negative = text.starts_with('-');
    let digits = text
        .strip_prefix('-')
        .or_else(|| text.strip_prefix('+'))
        .unwrap_or(text);
    let (digits, radix) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (hex, 16)
    } else if digits.starts_with('0') {
        (digits, 8)
    } else {
        (digits, 10)
    };
    if digits.starts_with('+') || digits.starts_with('-') {
        return None;
    }
    let value = u16::from_str_radix(digits, radix).ok()?;
    (value <= 999 && (!negative || value == 0)).then_some(value)
}
