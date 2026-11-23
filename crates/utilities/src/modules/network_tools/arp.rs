//! Lecture de la table ARP du système : associe adresses IP et adresses MAC
//! des machines du réseau local avec lesquelles on vient de communiquer.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::process::Command;

/// Lit la table ARP : `/proc/net/arp` sous Linux, sinon la commande `arp -a`
/// (Windows, macOS). Renvoie une table vide si rien n'est lisible.
pub fn read_arp_table() -> HashMap<Ipv4Addr, String> {
    if let Ok(text) = std::fs::read_to_string("/proc/net/arp") {
        return parse_arp(&text);
    }
    let mut command = Command::new("arp");
    command.arg("-a");
    hide_console_window(&mut command);
    match command.output() {
        Ok(output) => parse_arp(&String::from_utf8_lossy(&output.stdout)),
        Err(_) => HashMap::new(),
    }
}

/// Empêche l'apparition d'une fenêtre noire quand on lance une commande sous Windows.
pub fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = command;
}

/// Comprend les trois formats courants :
/// - Linux  `192.168.1.1  0x1  0x2  70:fc:8f:12:34:56  *  eth0`
/// - macOS  `? (192.168.1.1) at 70:fc:8f:12:34:56 on en0 ifscope [ethernet]`
/// - Windows `  192.168.1.1          70-fc-8f-12-34-56     dynamique`
pub fn parse_arp(text: &str) -> HashMap<Ipv4Addr, String> {
    let mut table = HashMap::new();
    for line in text.lines() {
        let mut ip = None;
        let mut mac = None;
        for token in line.split_whitespace() {
            let token = token.trim_matches(|c| c == '(' || c == ')');
            if ip.is_none() {
                if let Ok(address) = token.parse::<Ipv4Addr>() {
                    ip = Some(address);
                    continue;
                }
            }
            if mac.is_none() {
                mac = normalize_mac(token);
            }
        }
        if let (Some(ip), Some(mac)) = (ip, mac) {
            table.insert(ip, mac);
        }
    }
    table
}

/// `70-fc-8f-12-34-56` ou `0:1a:2b:3:4:5` → `70:FC:8F:12:34:56`.
/// Refuse les adresses nulles (entrée incomplète) et de diffusion.
fn normalize_mac(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split([':', '-']).collect();
    if parts.len() != 6 {
        return None;
    }
    let mut bytes = [0u8; 6];
    for (slot, part) in bytes.iter_mut().zip(&parts) {
        if part.is_empty() || part.len() > 2 {
            return None;
        }
        *slot = u8::from_str_radix(part, 16).ok()?;
    }
    if bytes.iter().all(|&b| b == 0) || bytes.iter().all(|&b| b == 0xff) {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    #[test]
    fn reads_the_linux_format_and_skips_incomplete_entries() {
        let text = "IP address       HW type     Flags       HW address            Mask     Device\n\
                    192.168.1.1      0x1         0x2         70:fc:8f:12:34:56     *        eth0\n\
                    192.168.1.9      0x1         0x0         00:00:00:00:00:00     *        eth0\n";
        let table = parse_arp(text);
        assert_eq!(table.len(), 1);
        assert_eq!(table[&ip("192.168.1.1")], "70:FC:8F:12:34:56");
    }

    #[test]
    fn reads_the_macos_format_with_short_octets() {
        let text = "? (192.168.1.20) at 0:1a:2b:3:4:5 on en0 ifscope [ethernet]\n\
                    ? (192.168.1.21) at (incomplete) on en0 ifscope [ethernet]\n";
        let table = parse_arp(text);
        assert_eq!(table.len(), 1);
        assert_eq!(table[&ip("192.168.1.20")], "00:1A:2B:03:04:05");
    }

    #[test]
    fn reads_the_windows_format_and_skips_broadcast() {
        let text = "Interface : 192.168.1.5 --- 0x7\n\
                    \x20 Adresse Internet     Adresse physique      Type\n\
                    \x20 192.168.1.1          70-fc-8f-12-34-56     dynamique\n\
                    \x20 192.168.1.255        ff-ff-ff-ff-ff-ff     statique\n";
        let table = parse_arp(text);
        assert_eq!(table.len(), 1);
        assert_eq!(table[&ip("192.168.1.1")], "70:FC:8F:12:34:56");
    }

    #[test]
    fn garbage_gives_an_empty_table() {
        assert!(parse_arp("rien à voir\n\n").is_empty());
    }
}
