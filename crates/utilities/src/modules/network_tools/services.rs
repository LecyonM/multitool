//! Noms des services courants associés aux numéros de port TCP.

const SERVICES: &[(u16, &str)] = &[
    (20, "FTP (données)"),
    (21, "FTP"),
    (22, "SSH"),
    (23, "Telnet"),
    (25, "SMTP"),
    (53, "DNS"),
    (67, "DHCP"),
    (80, "HTTP"),
    (88, "Kerberos"),
    (110, "POP3"),
    (111, "RPC"),
    (123, "NTP"),
    (135, "RPC Windows"),
    (139, "NetBIOS"),
    (143, "IMAP"),
    (161, "SNMP"),
    (389, "LDAP"),
    (443, "HTTPS"),
    (445, "Partage Windows (SMB)"),
    (465, "SMTPS"),
    (514, "Syslog"),
    (548, "AFP (partage Apple)"),
    (587, "SMTP (soumission)"),
    (631, "Impression (IPP)"),
    (636, "LDAPS"),
    (873, "rsync"),
    (993, "IMAPS"),
    (995, "POP3S"),
    (1433, "SQL Server"),
    (1521, "Oracle"),
    (1883, "MQTT"),
    (2049, "NFS"),
    (3000, "Application web (dev)"),
    (3306, "MySQL / MariaDB"),
    (3389, "Bureau à distance (RDP)"),
    (5000, "UPnP / application web"),
    (5353, "mDNS"),
    (5432, "PostgreSQL"),
    (5900, "VNC"),
    (6379, "Redis"),
    (8000, "HTTP (alternatif)"),
    (8080, "HTTP (alternatif)"),
    (8443, "HTTPS (alternatif)"),
    (9100, "Impression (JetDirect)"),
    (27017, "MongoDB"),
    (32400, "Plex"),
    (62078, "Synchronisation iPhone"),
];

/// Nom du service habituellement associé à ce port, ou « » s'il est inconnu.
pub fn service_name(port: u16) -> &'static str {
    SERVICES
        .iter()
        .find(|(p, _)| *p == port)
        .map_or("", |(_, name)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_ports() {
        assert_eq!(service_name(22), "SSH");
        assert_eq!(service_name(443), "HTTPS");
        assert_eq!(service_name(54321), "");
    }

    #[test]
    fn the_table_has_no_duplicate_port() {
        let mut ports: Vec<u16> = SERVICES.iter().map(|(p, _)| *p).collect();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(ports.len(), SERVICES.len());
    }
}
