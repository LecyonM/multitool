//! Mise en forme (en français) des tailles, vitesses et durées affichées sous la barre.

/// `1536` donne « 1,5 Ko ».
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    if count < 1024 {
        return format!("{count} o");
    }
    let mut value = count as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{} {}", decimal(value), UNITS[unit])
}

/// `3_355_443.0` donne « 3,2 Mo/s ».
pub fn speed(bytes_per_second: f64) -> String {
    format!("{}/s", bytes(bytes_per_second.max(0.0) as u64))
}

/// `65` donne « 1 min 05 s », `3725` donne « 1 h 02 min ».
pub fn duration(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min {:02} s", seconds / 60, seconds % 60),
        _ => format!("{} h {:02} min", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// Une décimale, virgule française.
fn decimal(value: f64) -> String {
    format!("{value:.1}").replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(bytes(0), "0 o");
        assert_eq!(bytes(1023), "1023 o");
        assert_eq!(bytes(1536), "1,5 Ko");
        assert_eq!(bytes(5 * 1024 * 1024), "5,0 Mo");
        assert_eq!(bytes(3 * 1024 * 1024 * 1024), "3,0 Go");
    }

    #[test]
    fn formats_speeds() {
        assert_eq!(speed(3.0 * 1024.0 * 1024.0), "3,0 Mo/s");
        assert_eq!(speed(-5.0), "0 o/s");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(7), "7 s");
        assert_eq!(duration(65), "1 min 05 s");
        assert_eq!(duration(3725), "1 h 02 min");
    }
}
