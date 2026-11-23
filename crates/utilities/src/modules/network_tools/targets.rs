//! Lecture des adresses et des ports saisis par l'utilisateur.

use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

use multitool_core::UtilityError;

/// Nombre maximal de machines pour un scan de réseau.
pub const MAX_HOSTS: usize = 4096;

fn invalid(message: impl Into<String>) -> UtilityError {
    UtilityError::InvalidInput(message.into())
}

/// Adresses à scanner : `192.168.1.0/24`, `192.168.1.10-50`,
/// `192.168.1.10-192.168.1.20`, une adresse seule ou un nom de machine.
/// Plusieurs éléments peuvent être séparés par des virgules ou des espaces.
pub fn parse_targets(spec: &str) -> Result<Vec<Ipv4Addr>, UtilityError> {
    let mut hosts = BTreeSet::new();
    let parts: Vec<&str> = spec
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(invalid(
            "Indiquez un réseau à scanner, par exemple 192.168.1.0/24.",
        ));
    }
    for part in parts {
        let (first, last) = if let Some((address, prefix)) = part.split_once('/') {
            cidr_bounds(part, address, prefix)?
        } else if let Some((start, end)) = part.split_once('-') {
            range_bounds(part, start, end)?
        } else {
            let single = single_address(part)?;
            (u32::from(single), u32::from(single))
        };
        let count = (last - first) as usize + 1;
        if hosts.len() + count > MAX_HOSTS {
            return Err(invalid(format!(
                "Trop d'adresses à scanner : le maximum est {MAX_HOSTS} machines par scan (l'équivalent d'un réseau /20). Réduisez la plage."
            )));
        }
        hosts.extend((first..=last).map(Ipv4Addr::from));
    }
    Ok(hosts.into_iter().collect())
}

/// Première et dernière adresse d'un réseau `a.b.c.d/n`.
/// Les adresses de réseau et de diffusion sont exclues (sauf /31 et /32).
fn cidr_bounds(whole: &str, address: &str, prefix: &str) -> Result<(u32, u32), UtilityError> {
    let address: Ipv4Addr = address
        .parse()
        .map_err(|_| invalid(format!("Adresse invalide : « {whole} ».")))?;
    let prefix: u32 = prefix
        .parse()
        .ok()
        .filter(|p| *p <= 32)
        .ok_or_else(|| invalid(format!("Masque invalide dans « {whole} » : utilisez un nombre de 0 à 32.")))?;
    let size = 1u64 << (32 - prefix);
    if size > (MAX_HOSTS as u64) + 2 {
        return Err(invalid(format!(
            "Le réseau « {whole} » contient {size} adresses : le maximum est {MAX_HOSTS} machines par scan (/20). Choisissez un réseau plus petit, par exemple /24."
        )));
    }
    let mask = if prefix == 0 { 0 } else { u32::MAX << (32 - prefix) };
    let base = u32::from(address) & mask;
    let top = base + (size as u32 - 1);
    Ok(if prefix <= 30 {
        (base + 1, top - 1)
    } else {
        (base, top)
    })
}

/// `a.b.c.d-e` (dernier nombre seul) ou `a.b.c.d-w.x.y.z`.
fn range_bounds(whole: &str, start: &str, end: &str) -> Result<(u32, u32), UtilityError> {
    let start: Ipv4Addr = start
        .parse()
        .map_err(|_| invalid(format!("Plage invalide : « {whole} ».")))?;
    let end: Ipv4Addr = if let Ok(full) = end.parse::<Ipv4Addr>() {
        full
    } else {
        let last: u8 = end
            .parse()
            .map_err(|_| invalid(format!("Plage invalide : « {whole} ».")))?;
        let [a, b, c, _] = start.octets();
        Ipv4Addr::new(a, b, c, last)
    };
    let (first, last) = (u32::from(start), u32::from(end));
    if last < first {
        return Err(invalid(format!(
            "Plage invalide : « {whole} » se termine avant son début."
        )));
    }
    if (last - first) as usize >= MAX_HOSTS {
        return Err(invalid(format!(
            "La plage « {whole} » est trop grande : le maximum est {MAX_HOSTS} machines par scan."
        )));
    }
    Ok((first, last))
}

fn single_address(text: &str) -> Result<Ipv4Addr, UtilityError> {
    if let Ok(address) = text.parse::<Ipv4Addr>() {
        return Ok(address);
    }
    match resolve(text) {
        Some(IpAddr::V4(address)) => Ok(address),
        Some(IpAddr::V6(_)) => Err(invalid(format!(
            "« {text} » n'a qu'une adresse IPv6 : le scanner de réseau ne gère que l'IPv4."
        ))),
        None => Err(invalid(format!(
            "« {text} » n'est ni une adresse IP ni un nom de machine connu."
        ))),
    }
}

/// Adresse d'un nom de machine (IPv4 de préférence).
pub fn resolve(host: &str) -> Option<IpAddr> {
    let addresses: Vec<IpAddr> = (host, 0u16)
        .to_socket_addrs()
        .ok()?
        .map(|socket| socket.ip())
        .collect();
    addresses
        .iter()
        .find(|a| a.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
}

/// Ports à tester : `1-1024`, `22,80,443`, `20-25,80` ou `tous` (1 à 65535).
pub fn parse_ports(spec: &str) -> Result<Vec<u16>, UtilityError> {
    let spec = spec.trim().to_lowercase();
    if matches!(spec.as_str(), "tous" | "tout" | "all" | "*") {
        return Ok((1..=u16::MAX).collect());
    }
    let mut ports = BTreeSet::new();
    let parts: Vec<&str> = spec
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(invalid(
            "Indiquez les ports à tester, par exemple 1-1024, 22,80,443 ou « tous ».",
        ));
    }
    for part in parts {
        let (first, last) = match part.split_once('-') {
            Some((a, b)) => (port_number(part, a)?, port_number(part, b)?),
            None => {
                let single = port_number(part, part)?;
                (single, single)
            }
        };
        if last < first {
            return Err(invalid(format!(
                "Plage de ports invalide : « {part} » se termine avant son début."
            )));
        }
        ports.extend(first..=last);
    }
    Ok(ports.into_iter().collect())
}

fn port_number(whole: &str, text: &str) -> Result<u16, UtilityError> {
    text.parse::<u16>()
        .ok()
        .filter(|port| *port >= 1)
        .ok_or_else(|| {
            invalid(format!(
                "Port invalide dans « {whole} » : utilisez des nombres de 1 à 65535."
            ))
        })
}

/// Vrai pour les adresses du réseau local, de la machine elle-même ou d'un réseau privé.
pub fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let first = v6.segments()[0];
            v6.is_loopback()
                || (first & 0xfe00) == 0xfc00 // fc00::/7 : adresses locales uniques
                || (first & 0xffc0) == 0xfe80 // fe80::/10 : lien local
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    fn message(error: UtilityError) -> String {
        error.to_string()
    }

    #[test]
    fn a_slash_24_gives_254_hosts_without_network_and_broadcast() {
        let hosts = parse_targets("192.168.1.0/24").unwrap();
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], ip("192.168.1.1"));
        assert_eq!(hosts[253], ip("192.168.1.254"));
    }

    #[test]
    fn small_networks_and_single_hosts() {
        assert_eq!(parse_targets("10.0.0.0/30").unwrap().len(), 2);
        assert_eq!(parse_targets("10.0.0.4/31").unwrap().len(), 2);
        assert_eq!(parse_targets("10.0.0.7/32").unwrap(), vec![ip("10.0.0.7")]);
        assert_eq!(parse_targets("10.0.0.7").unwrap(), vec![ip("10.0.0.7")]);
    }

    #[test]
    fn the_host_bits_of_a_cidr_are_ignored() {
        let hosts = parse_targets("192.168.1.77/24").unwrap();
        assert_eq!(hosts[0], ip("192.168.1.1"));
    }

    #[test]
    fn ranges_in_short_and_long_form() {
        let short = parse_targets("192.168.1.10-14").unwrap();
        assert_eq!(short.len(), 5);
        assert_eq!(short[4], ip("192.168.1.14"));
        let long = parse_targets("192.168.1.250-192.168.2.2").unwrap();
        assert_eq!(long.len(), 9);
    }

    #[test]
    fn lists_are_merged_and_deduplicated_and_sorted() {
        let hosts = parse_targets("192.168.1.5, 192.168.1.3 192.168.1.5;192.168.1.1-4").unwrap();
        assert_eq!(
            hosts,
            vec![
                ip("192.168.1.1"),
                ip("192.168.1.2"),
                ip("192.168.1.3"),
                ip("192.168.1.4"),
                ip("192.168.1.5")
            ]
        );
    }

    #[test]
    fn localhost_is_resolved() {
        assert_eq!(parse_targets("localhost").unwrap(), vec![ip("127.0.0.1")]);
    }

    #[test]
    fn bad_input_gets_a_clear_french_message() {
        assert!(message(parse_targets("").unwrap_err()).contains("Indiquez"));
        assert!(message(parse_targets("192.168.1.0/33").unwrap_err()).contains("Masque invalide"));
        assert!(message(parse_targets("192.168.1.300/24").unwrap_err()).contains("Adresse invalide"));
        assert!(message(parse_targets("192.168.1.20-10").unwrap_err()).contains("avant son début"));
        assert!(message(parse_targets("192.168.1.1-abc").unwrap_err()).contains("Plage invalide"));
        assert!(message(parse_targets("introuvable.invalid").unwrap_err()).contains("ni une adresse IP"));
    }

    #[test]
    fn huge_networks_are_refused() {
        let error = message(parse_targets("10.0.0.0/8").unwrap_err());
        assert!(error.contains("maximum"), "{error}");
        assert!(parse_targets("10.0.0.0/20").is_ok());
        assert!(parse_targets("10.0.0.0/19").is_err());
        assert!(parse_targets("10.0.0.0/24 10.0.1.0/20").is_err());
    }

    #[test]
    fn ports_lists_ranges_and_all() {
        assert_eq!(parse_ports("80").unwrap(), vec![80]);
        assert_eq!(parse_ports("443, 22 80,20-22").unwrap(), vec![20, 21, 22, 80, 443]);
        assert_eq!(parse_ports("TOUS").unwrap().len(), 65535);
        assert_eq!(parse_ports("1-1024").unwrap().len(), 1024);
    }

    #[test]
    fn invalid_ports_are_refused() {
        assert!(parse_ports("").is_err());
        assert!(parse_ports("0").is_err());
        assert!(parse_ports("70000").is_err());
        assert!(parse_ports("100-50").is_err());
        assert!(parse_ports("http").is_err());
    }

    #[test]
    fn private_and_public_addresses() {
        for private in ["192.168.1.1", "10.1.2.3", "172.16.0.9", "127.0.0.1", "169.254.1.1", "::1", "fd00::1", "fe80::1"] {
            assert!(is_private(private.parse().unwrap()), "{private}");
        }
        for public in ["8.8.8.8", "1.1.1.1", "172.32.0.1", "2001:4860:4860::8888"] {
            assert!(!is_private(public.parse().unwrap()), "{public}");
        }
    }
}
