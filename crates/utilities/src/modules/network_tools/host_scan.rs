//! « Scanner de réseau » : liste les machines allumées d'un réseau, façon `nmap -sn 192.168.1.0/24`.
//!
//! Sans droits administrateur, une machine est déclarée active si :
//! - elle répond au `ping` du système ;
//! - ou accepte / refuse une connexion TCP sur un port courant (un refus prouve qu'elle est là) ;
//! - ou figure dans la table ARP, remplie par ces tentatives (réseau local uniquement).

use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use multitool_core::{
    Field, Grid, Input, Output, ProgressUpdate, Utility, UtilityError, UtilityInfo,
};
use serde_json::json;

use super::arp::{hide_console_window, read_arp_table};
use super::pool::run_parallel;
use super::targets::{is_private, parse_targets};
use super::format_duration;

/// Ports essayés sur chaque machine qui ne répond pas au ping.
const PROBE_PORTS: [u16; 8] = [80, 443, 22, 445, 139, 135, 3389, 8080];
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
const WORKERS: usize = 128;

pub struct NetworkScanner;

impl Utility for NetworkScanner {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "network_scanner",
            "Scanner de réseau",
            "Détecte les machines allumées sur un réseau (ping, connexions TCP, table ARP), comme « nmap -sn 192.168.1.0/24 ».",
            "Réseau",
        )
    }

    fn fields(&self) -> Vec<Field> {
        vec![
            Field::text("target", "Réseau ou adresses à scanner")
                .placeholder("192.168.1.0/24, 192.168.1.10-50 ou 192.168.1.20")
                .default_value(json!(guess_local_network())),
            Field::checkbox("names", "Chercher aussi le nom des machines (plus lent)")
                .default_value(json!(true)),
            Field::checkbox(
                "confirm_public",
                "J'ai l'autorisation de scanner des adresses hors de mon réseau local",
            ),
        ]
    }

    fn run(&self, input: &Input) -> Result<Output, UtilityError> {
        self.run_with_progress(input, &|_| {})
    }

    fn run_with_progress(
        &self,
        input: &Input,
        progress: &dyn Fn(ProgressUpdate),
    ) -> Result<Output, UtilityError> {
        let targets = parse_targets(&input.string("target"))?;
        if !input.bool("confirm_public")
            && targets.iter().any(|ip| !is_private(IpAddr::V4(*ip)))
        {
            return Err(UtilityError::InvalidInput(
                "Certaines adresses ne sont pas dans un réseau privé. Scannez uniquement des machines qui vous appartiennent ou dont vous avez l'autorisation, puis cochez la case d'autorisation.".into(),
            ));
        }
        let started = Instant::now();
        let total = targets.len();

        progress(ProgressUpdate::new(0.0, format!("0 machine testée sur {total}")));
        let found: Mutex<Vec<(Ipv4Addr, String)>> = Mutex::new(Vec::new());
        run_parallel(
            total,
            WORKERS,
            |index| {
                if let Some(via) = probe(targets[index]) {
                    found.lock().unwrap().push((targets[index], via));
                }
            },
            |done| {
                let count = found.lock().unwrap().len();
                progress(ProgressUpdate::new(
                    done as f64 / total as f64 * 0.9,
                    format!("{done} machines testées sur {total} : {count} trouvée(s)"),
                ));
            },
        );
        let mut found = found.into_inner().unwrap();

        // La table ARP révèle aussi les machines qui bloquent ping et TCP (réseau local).
        let arp = read_arp_table();
        for ip in &targets {
            if arp.contains_key(ip) && !found.iter().any(|(known, _)| known == ip) {
                found.push((*ip, "ARP".into()));
            }
        }
        found.sort_by_key(|(ip, _)| *ip);

        let names: Mutex<HashMap<Ipv4Addr, String>> = Mutex::new(HashMap::new());
        if input.bool("names") && !found.is_empty() {
            let count = found.len();
            run_parallel(
                count,
                32,
                |index| {
                    let ip = found[index].0;
                    if let Some(name) = reverse_name(ip) {
                        names.lock().unwrap().insert(ip, name);
                    }
                },
                |done| {
                    progress(ProgressUpdate::new(
                        0.9 + done as f64 / count as f64 * 0.1,
                        format!("Recherche des noms : {done} sur {count}"),
                    ));
                },
            );
        }
        let names = names.into_inner().unwrap();

        let mut grid = Grid::new(&["Adresse IP", "Nom", "Adresse MAC", "Détectée par"]);
        for (ip, via) in &found {
            grid.push(vec![
                ip.to_string(),
                names.get(ip).cloned().unwrap_or_default(),
                arp.get(ip).cloned().unwrap_or_default(),
                via.clone(),
            ]);
        }

        let mut output = Output::table(vec![
            ("Machines actives".to_string(), format!("{} sur {total}", found.len())),
            ("Durée".to_string(), format_duration(started.elapsed().as_secs_f64())),
        ]);
        output = if found.is_empty() {
            output.with_text("Aucune machine n'a répondu. Vérifiez l'adresse du réseau et votre connexion ; une machine protégée par un pare-feu strict peut rester invisible.")
        } else {
            output.with_grid(grid).with_text(
                "Les adresses MAC ne sont visibles que sur votre réseau local. Une machine dont le pare-feu bloque tout peut ne pas apparaître.",
            )
        };
        progress(ProgressUpdate::new(1.0, "Scan terminé"));
        Ok(output)
    }
}

/// Premier moyen qui prouve que la machine est là : `ping`, puis connexion TCP.
fn probe(ip: Ipv4Addr) -> Option<String> {
    if ping(ip) {
        return Some("ping".into());
    }
    for port in PROBE_PORTS {
        match TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(ip), port), PROBE_TIMEOUT) {
            Ok(_) => return Some(format!("TCP {port}")),
            // Un refus (RST) ne peut venir que de la machine elle-même.
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                return Some(format!("TCP {port}"))
            }
            Err(_) => {}
        }
    }
    None
}

static PING_MISSING: AtomicBool = AtomicBool::new(false);

/// Utilise la commande `ping` du système (pas besoin de droits spéciaux).
fn ping(ip: Ipv4Addr) -> bool {
    if PING_MISSING.load(Ordering::Relaxed) {
        return false;
    }
    let mut command = Command::new("ping");
    if cfg!(windows) {
        command.args(["-n", "1", "-w", "800"]);
    } else if cfg!(target_os = "macos") {
        command.args(["-c", "1", "-W", "800"]);
    } else {
        command.args(["-c", "1", "-W", "1"]);
    }
    command.arg(ip.to_string());
    hide_console_window(&mut command);
    match command.output() {
        // Windows répond parfois « hôte injoignable » avec un code de succès : on exige un TTL.
        Ok(output) if cfg!(windows) => {
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("TTL=")
        }
        Ok(output) => output.status.success(),
        Err(error) => {
            if error.kind() == ErrorKind::NotFound {
                PING_MISSING.store(true, Ordering::Relaxed);
            }
            false
        }
    }
}

fn reverse_name(ip: Ipv4Addr) -> Option<String> {
    let name = dns_lookup::lookup_addr(&IpAddr::V4(ip)).ok()?;
    (name != ip.to_string() && !name.is_empty()).then_some(name)
}

/// Réseau /24 de cette machine (sans envoyer de paquet), ou un exemple courant.
fn guess_local_network() -> String {
    let local = UdpSocket::bind("0.0.0.0:0").ok().and_then(|socket| {
        socket.connect("192.0.2.1:9").ok()?;
        socket.local_addr().ok()
    });
    match local.map(|address| address.ip()) {
        Some(IpAddr::V4(ip)) if !ip.is_loopback() && !ip.is_unspecified() => {
            let [a, b, c, _] = ip.octets();
            format!("{a}.{b}.{c}.0/24")
        }
        _ => "192.168.1.0/24".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn a_machine_with_an_open_port_is_found_with_its_name() {
        // 127.0.0.1 répond toujours (ping ou refus de connexion) : machine « active ».
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _keep = listener;
        let input = Input::new().with("target", "127.0.0.1").with("names", false);
        let output = NetworkScanner.run(&input).unwrap();
        assert_eq!(output.table[0].value, "1 sur 1");
        let grid = output.grid.unwrap();
        assert_eq!(grid.rows.len(), 1);
        assert_eq!(grid.rows[0][0], "127.0.0.1");
        assert!(!grid.rows[0][3].is_empty());
    }

    #[test]
    fn progress_ends_at_100_percent() {
        let last = Mutex::new(None);
        let input = Input::new().with("target", "127.0.0.1").with("names", false);
        NetworkScanner
            .run_with_progress(&input, &|update| *last.lock().unwrap() = update.fraction)
            .unwrap();
        assert_eq!(*last.lock().unwrap(), Some(1.0));
    }

    #[test]
    fn public_addresses_need_the_confirmation_box() {
        let input = Input::new().with("target", "8.8.8.8");
        let error = NetworkScanner.run(&input).unwrap_err().to_string();
        assert!(error.contains("autorisation"), "{error}");
    }

    #[test]
    fn a_bad_network_is_reported() {
        let input = Input::new().with("target", "192.168.1.0/99");
        assert!(NetworkScanner.run(&input).is_err());
    }

    #[test]
    fn the_default_target_is_a_valid_network() {
        let fields = NetworkScanner.fields();
        let default = fields[0].default.as_ref().unwrap().as_str().unwrap();
        assert!(parse_targets(default).is_ok(), "{default}");
    }
}
