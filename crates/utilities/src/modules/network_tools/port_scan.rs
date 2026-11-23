//! « Scanner de ports » : cherche les ports TCP ouverts d'une machine (jusqu'aux 65 535).
//!
//! Chaque port reçoit un verdict :
//! - **ouvert** : la connexion aboutit ;
//! - **fermé** : la machine refuse aussitôt la connexion ;
//! - **sans réponse** : aucune réponse avant le délai (pare-feu, machine éteinte).

use std::io::{ErrorKind, Read};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use multitool_core::{
    Field, Grid, Input, Output, ProgressUpdate, Utility, UtilityError, UtilityInfo,
};
use serde_json::json;

use super::format_duration;
use super::pool::run_parallel;
use super::services::service_name;
use super::targets::{is_private, parse_ports, resolve};

/// Reste sous la limite de 256 fichiers ouverts par défaut de macOS.
const WORKERS: usize = 128;

pub struct PortScanner;

enum Verdict {
    Open(String),
    Closed,
    NoAnswer,
}

impl Utility for PortScanner {
    fn info(&self) -> UtilityInfo {
        UtilityInfo::new(
            "port_scanner",
            "Scanner de ports",
            "Teste les ports TCP d'une machine (de quelques-uns aux 65 535) et liste ceux qui sont ouverts.",
            "Réseau",
        )
    }

    fn fields(&self) -> Vec<Field> {
        vec![
            Field::text("host", "Adresse IP ou nom de la machine")
                .placeholder("192.168.1.10 ou 127.0.0.1"),
            Field::text("ports", "Ports à tester (1-1024, 22,80,443 ou « tous »)")
                .default_value(json!("1-1024")),
            Field::number("timeout", "Délai d'attente par port (millisecondes)")
                .range(50.0, 10_000.0)
                .default_value(json!(600)),
            Field::checkbox(
                "confirm_public",
                "J'ai l'autorisation de scanner cette machine (obligatoire hors réseau local)",
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
        let host = input.string("host");
        let host = host.trim();
        if host.is_empty() {
            return Err(UtilityError::InvalidInput(
                "Indiquez l'adresse IP ou le nom de la machine à scanner.".into(),
            ));
        }
        let address = host
            .parse::<IpAddr>()
            .ok()
            .or_else(|| resolve(host))
            .ok_or_else(|| {
                UtilityError::InvalidInput(format!(
                    "« {host} » n'est ni une adresse IP ni un nom de machine connu."
                ))
            })?;
        if !is_private(address) && !input.bool("confirm_public") {
            return Err(UtilityError::InvalidInput(format!(
                "{address} n'est pas dans un réseau privé. Scannez uniquement une machine qui vous appartient ou dont vous avez l'autorisation, puis cochez la case d'autorisation."
            )));
        }
        let ports = parse_ports(&input.string("ports"))?;
        let timeout = Duration::from_millis(
            input
                .number("timeout")
                .filter(|ms| (50.0..=10_000.0).contains(ms))
                .unwrap_or(600.0) as u64,
        );

        let total = ports.len();
        let started = Instant::now();
        let open: Mutex<Vec<(u16, String)>> = Mutex::new(Vec::new());
        let closed = AtomicUsize::new(0);
        let no_answer = AtomicUsize::new(0);

        progress(ProgressUpdate::new(0.0, format!("0 port testé sur {total}")));
        run_parallel(
            total,
            WORKERS,
            |index| {
                let port = ports[index];
                match check(SocketAddr::new(address, port), timeout) {
                    Verdict::Open(banner) => open.lock().unwrap().push((port, banner)),
                    Verdict::Closed => {
                        closed.fetch_add(1, Ordering::Relaxed);
                    }
                    Verdict::NoAnswer => {
                        no_answer.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            |done| {
                let count = open.lock().unwrap().len();
                progress(ProgressUpdate::new(
                    done as f64 / total as f64,
                    format!("{done} ports testés sur {total} : {count} ouvert(s)"),
                ));
            },
        );

        let mut open = open.into_inner().unwrap();
        open.sort_by_key(|(port, _)| *port);
        let no_answer = no_answer.into_inner();

        let mut output = Output::table(vec![
            ("Machine".to_string(), address.to_string()),
            ("Ports testés".to_string(), total.to_string()),
            ("Ports ouverts".to_string(), open.len().to_string()),
            ("Ports fermés".to_string(), closed.into_inner().to_string()),
            ("Sans réponse".to_string(), no_answer.to_string()),
            ("Durée".to_string(), format_duration(started.elapsed().as_secs_f64())),
        ]);
        if open.is_empty() {
            let reason = if no_answer == total {
                "Aucune réponse : la machine est éteinte, injoignable ou protégée par un pare-feu."
            } else {
                "Aucun port ouvert parmi ceux testés."
            };
            output = output.with_text(reason);
        } else {
            let mut grid = Grid::new(&["Port", "Service", "Aperçu"]);
            for (port, banner) in &open {
                grid.push(vec![
                    format!("{port}/tcp"),
                    service_name(*port).to_string(),
                    banner.clone(),
                ]);
            }
            output = output.with_grid(grid);
            if no_answer > 0 {
                output = output.with_text(
                    "Les ports « sans réponse » sont probablement filtrés par un pare-feu : ils ne sont pas forcément fermés.",
                );
            }
        }
        progress(ProgressUpdate::new(1.0, "Scan terminé"));
        Ok(output)
    }
}

fn check(address: SocketAddr, timeout: Duration) -> Verdict {
    match TcpStream::connect_timeout(&address, timeout) {
        Ok(stream) => Verdict::Open(read_banner(stream)),
        Err(error) if error.kind() == ErrorKind::ConnectionRefused => Verdict::Closed,
        Err(_) => Verdict::NoAnswer,
    }
}

/// Première ligne annoncée spontanément par le service (SSH, FTP, SMTP…), le cas échéant.
fn read_banner(mut stream: TcpStream) -> String {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(150)));
    let mut buffer = [0u8; 128];
    let Ok(read) = stream.read(&mut buffer) else {
        return String::new();
    };
    let text: String = String::from_utf8_lossy(&buffer[..read])
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(60)
        .collect();
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    /// Serveur local qui annonce une bannière, comme un serveur SSH.
    fn spawn_server(banner: &'static str) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let _ = stream.write_all(banner.as_bytes());
            }
        });
        port
    }

    fn unused_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn finds_the_open_port_and_its_banner_and_counts_the_closed_one() {
        let open = spawn_server("SSH-2.0-TestServer\r\nsuite ignorée\r\n");
        let closed = unused_port();
        let input = Input::new()
            .with("host", "127.0.0.1")
            .with("ports", format!("{open},{closed}"))
            .with("timeout", 500);
        let output = NetworkScannerless::run(&input);
        let value = |label: &str| {
            output
                .table
                .iter()
                .find(|e| e.label == label)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(value("Ports testés"), "2");
        assert_eq!(value("Ports ouverts"), "1");
        assert_eq!(value("Ports fermés"), "1");
        let grid = output.grid.unwrap();
        assert_eq!(grid.rows[0][0], format!("{open}/tcp"));
        assert_eq!(grid.rows[0][2], "SSH-2.0-TestServer");
    }

    #[test]
    fn open_ports_are_sorted() {
        let a = spawn_server("");
        let b = spawn_server("");
        let (low, high) = (a.min(b), a.max(b));
        let input = Input::new()
            .with("host", "127.0.0.1")
            .with("ports", format!("{high},{low}"));
        let grid = NetworkScannerless::run(&input).grid.unwrap();
        assert_eq!(grid.rows[0][0], format!("{low}/tcp"));
        assert_eq!(grid.rows[1][0], format!("{high}/tcp"));
    }

    #[test]
    fn a_closed_range_reports_no_open_port() {
        let closed = unused_port();
        let input = Input::new()
            .with("host", "localhost")
            .with("ports", closed.to_string());
        let output = NetworkScannerless::run(&input);
        assert!(output.grid.is_none());
        assert!(output.text.contains("Aucun port ouvert"));
    }

    #[test]
    fn a_full_scan_of_the_whole_port_range_works_and_reports_progress() {
        let open = spawn_server("");
        let last = Mutex::new(None);
        let input = Input::new()
            .with("host", "127.0.0.1")
            .with("ports", "tous")
            .with("timeout", 500);
        let output = PortScanner
            .run_with_progress(&input, &|update| *last.lock().unwrap() = update.fraction)
            .unwrap();
        assert_eq!(*last.lock().unwrap(), Some(1.0));
        assert_eq!(output.table[1].value, "65535");
        let grid = output.grid.unwrap();
        assert!(grid.rows.iter().any(|row| row[0] == format!("{open}/tcp")));
    }

    #[test]
    fn invalid_input_is_refused_with_a_clear_message() {
        let run = |input: Input| PortScanner.run(&input).unwrap_err().to_string();
        assert!(run(Input::new()).contains("Indiquez"));
        assert!(run(Input::new().with("host", "introuvable.invalid")).contains("ni une adresse IP"));
        assert!(run(Input::new().with("host", "127.0.0.1").with("ports", "abc")).contains("Port invalide"));
        assert!(run(Input::new().with("host", "8.8.8.8").with("ports", "53")).contains("autorisation"));
    }

    struct NetworkScannerless;
    impl NetworkScannerless {
        fn run(input: &Input) -> Output {
            PortScanner.run(input).unwrap()
        }
    }
}
