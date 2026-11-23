//! Outils réseau : scanner de réseau (machines allumées) et scanner de ports.
//!
//! Tout fonctionne **sans droits administrateur** : détection par `ping` du système,
//! table ARP et connexions TCP ordinaires (pas de paquets bruts comme avec `nmap -sS`).
//!
//! - `targets.rs`   : lecture des adresses (CIDR, plages, noms) et des ports ;
//! - `arp.rs`       : lecture de la table ARP (adresses MAC) ;
//! - `pool.rs`      : exécution en parallèle avec suivi d'avancement ;
//! - `services.rs`  : noms des services courants (22 → SSH…) ;
//! - `host_scan.rs` : l'utilitaire « Scanner de réseau » ;
//! - `port_scan.rs` : l'utilitaire « Scanner de ports ».

mod arp;
mod host_scan;
mod pool;
mod port_scan;
mod services;
mod targets;

use multitool_core::Registry;

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(host_scan::NetworkScanner));
    registry.register(Box::new(port_scan::PortScanner));
}

/// Durée lisible en français : « 4,2 s » ou « 2 min 05 s ».
fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{seconds:.1} s").replace('.', ",")
    } else {
        let total = seconds.round() as u64;
        format!("{} min {:02} s", total / 60, total % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_are_formatted_in_french() {
        assert_eq!(format_duration(4.24), "4,2 s");
        assert_eq!(format_duration(125.0), "2 min 05 s");
    }
}
