use anyhow::{Result, anyhow};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

pub fn send_keys(chord: &str) -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow!("failed to initialize input: {e}"))?;

    let tokens: Vec<&str> = chord.split('+').map(str::trim).collect();

    if tokens.is_empty() {
        return Err(anyhow!("empty key chord"));
    }

    let (modifier_tokens, main_token) = tokens.split_at(tokens.len() - 1);

    let modifiers: Vec<Key> = modifier_tokens
        .iter()
        .map(|s| parse_modifier(s))
        .collect::<Result<Vec<_>>>()?;

    let main_key = parse_key(main_token[0])?;

    for &modifier in &modifiers {
        enigo
            .key(modifier, Direction::Press)
            .map_err(|e| anyhow!("key press failed: {e}"))?;
    }

    enigo
        .key(main_key, Direction::Click)
        .map_err(|e| anyhow!("key click failed: {e}"))?;

    for &modifier in modifiers.iter().rev() {
        enigo
            .key(modifier, Direction::Release)
            .map_err(|e| anyhow!("key release failed: {e}"))?;
    }

    Ok(())
}

fn parse_modifier(s: &str) -> Result<Key> {
    let key = parse_key(s)?;
    match key {
        Key::Control | Key::Alt | Key::Shift | Key::Meta => Ok(key),
        _ => Err(anyhow!("'{}' is not a valid modifier key", s)),
    }
}

fn parse_key(s: &str) -> Result<Key> {
    match s.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Ok(Key::Control),
        "alt" => Ok(Key::Alt),
        "shift" => Ok(Key::Shift),
        "win" | "super" | "meta" | "cmd" => Ok(Key::Meta),
        "space" => Ok(Key::Space),
        "enter" | "return" => Ok(Key::Return),
        "tab" => Ok(Key::Tab),
        "backspace" => Ok(Key::Backspace),
        "escape" | "esc" => Ok(Key::Escape),
        "delete" | "del" => Ok(Key::Delete),
        "home" => Ok(Key::Home),
        "end" => Ok(Key::End),
        "pageup" | "pgup" => Ok(Key::PageUp),
        "pagedown" | "pgdn" | "pgdown" => Ok(Key::PageDown),
        "left" => Ok(Key::LeftArrow),
        "right" => Ok(Key::RightArrow),
        "up" => Ok(Key::UpArrow),
        "down" => Ok(Key::DownArrow),
        "f1" => Ok(Key::F1),
        "f2" => Ok(Key::F2),
        "f3" => Ok(Key::F3),
        "f4" => Ok(Key::F4),
        "f5" => Ok(Key::F5),
        "f6" => Ok(Key::F6),
        "f7" => Ok(Key::F7),
        "f8" => Ok(Key::F8),
        "f9" => Ok(Key::F9),
        "f10" => Ok(Key::F10),
        "f11" => Ok(Key::F11),
        "f12" => Ok(Key::F12),
        s if s.chars().count() == 1 => Ok(Key::Unicode(s.chars().next().unwrap())),
        other => Err(anyhow!("unknown key '{}'", other)),
    }
}
