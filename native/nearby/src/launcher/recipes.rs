use crate::platform::content;
use crate::{Result, require};
use std::path::Path;

pub use content::{Launch, finish, prepare};

pub fn configuration_context(path: &Path) -> Result<crate::context::Context> {
    let launch = content::recipe(path)?;
    let words = command_words(&launch.command)?;
    let (index, frontend) = words
        .iter()
        .enumerate()
        .find_map(|(index, word)| {
            let name = Path::new(word).file_name()?.to_str()?;
            matches!(name, "retroarch" | "retroarch32").then_some((index, name))
        })
        .ok_or("此启动方式使用独立模拟器")?;
    let args = words
        .iter()
        .skip(index + 1)
        .take_while(|word| word.as_str() != ";")
        .cloned()
        .collect::<Vec<_>>();
    crate::context::parse(frontend, &args)
}

fn command_words(command: &str) -> Result<Vec<String>> {
    let mut words: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut quote = None;
    let mut escape = false;
    let mut dynamic = false;
    for ch in command.chars() {
        if escape {
            text.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && quote != Some('\'') {
            escape = true;
            continue;
        }
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            } else {
                if mark == '"' && matches!(ch, '$' | '`') {
                    dynamic = true;
                }
                text.push(ch);
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
        } else if ch == ';' || ch.is_whitespace() {
            if !text.is_empty() {
                words.push(std::mem::take(&mut text));
            }
            if ch == ';' {
                if words.iter().any(|word| {
                    Path::new(word)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| matches!(name, "retroarch" | "retroarch32"))
                }) {
                    require(!dynamic, "模拟器命令包含动态 shell 参数")?;
                    return Ok(words);
                }
                words.clear();
                dynamic = false;
            }
        } else {
            dynamic |= matches!(ch, '`' | '$' | '&' | '|');
            text.push(ch);
        }
    }
    require(quote.is_none() && !escape, "启动配方引号不完整")?;
    if !text.is_empty() {
        words.push(text);
    }
    require(!dynamic, "模拟器命令包含动态 shell 参数")?;
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configuration_arguments_keep_quoted_content_and_shell_boundaries() {
        let words = command_words("sudo retroarch -c 'config one.cfg' --appendconfig='a.cfg|b.cfg' -L snes9x '/roms/a'\\''b $(x).zip'; perfnorm").unwrap();
        assert_eq!(
            words,
            [
                "sudo",
                "retroarch",
                "-c",
                "config one.cfg",
                "--appendconfig=a.cfg|b.cfg",
                "-L",
                "snes9x",
                "/roms/a'b $(x).zip",
            ]
        );
        assert!(command_words("retroarch $(date)").is_err());
        assert!(command_words("retroarch \"$HOME/game.zip\"").is_err());
        assert_eq!(
            command_words("before=$?; retroarch -L core 'game.zip'; after=$?; exit \"$after\"")
                .unwrap(),
            ["retroarch", "-L", "core", "game.zip"]
        );
        assert!(command_words("retroarch 'unclosed").is_err());
    }
}
