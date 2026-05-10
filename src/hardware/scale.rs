//! Scale Hardware Service — Weighing scale integration
//!
//! Supports CAS, Toledo (MT-SICS), and generic serial scale protocols.
//! Uses a `tokio::sync::watch` channel to push the latest reading to the UI.
//! Feature-gated behind `hardware`.

use rust_decimal::Decimal;
use std::str::FromStr;

/// A single weight reading from the scale
#[derive(Clone, Debug)]
pub struct ScaleReading {
    /// Weight value (e.g. 0.450)
    pub weight: Decimal,
    /// Unit of the reading
    pub unit: ScaleUnit,
    /// Whether the reading is stable (motion has stopped)
    pub stable: bool,
    /// Raw data string from the scale
    pub raw: String,
}

impl Default for ScaleReading {
    fn default() -> Self {
        Self {
            weight: Decimal::ZERO,
            unit: ScaleUnit::Kg,
            stable: false,
            raw: String::new(),
        }
    }
}

/// Unit reported by the scale
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScaleUnit {
    Kg,
    Gram,
    Lb,
}

impl ScaleUnit {
    /// Localized label
    pub fn label(&self, locale: &str) -> &'static str {
        match (self, locale) {
            (ScaleUnit::Kg, "ar") => "كجم",
            (ScaleUnit::Kg, _) => "kg",
            (ScaleUnit::Gram, "ar") => "جرام",
            (ScaleUnit::Gram, _) => "g",
            (ScaleUnit::Lb, "ar") => "رطل",
            (ScaleUnit::Lb, _) => "lb",
        }
    }
}

/// Current status of the scale connection
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScaleStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// Scale configuration (parsed from config TOML)
#[derive(Clone, Debug)]
pub struct ScaleConfig {
    pub enabled: bool,
    pub port: String,
    pub baud_rate: u32,
    pub protocol: ScaleProtocol,
}

impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: "/dev/ttyUSB2".to_string(),
            baud_rate: 9600,
            protocol: ScaleProtocol::Cas,
        }
    }
}

/// Supported scale protocols
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScaleProtocol {
    Cas,
    Toledo,
    Generic,
}

impl FromStr for ScaleProtocol {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cas" => Ok(ScaleProtocol::Cas),
            "toledo" | "mt-sics" | "mtsics" => Ok(ScaleProtocol::Toledo),
            _ => Ok(ScaleProtocol::Generic),
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol parsers
// ---------------------------------------------------------------------------

/// Trait for parsing raw scale data into readings
pub trait ProtocolParser: Send + Sync {
    /// Attempt to parse a line of data from the scale
    fn parse(&self, data: &[u8]) -> Option<ScaleReading>;
    /// Command bytes to tare the scale
    fn tare_command(&self) -> Vec<u8>;
    /// Command bytes to zero the scale
    fn zero_command(&self) -> Vec<u8>;
}

/// CAS scale protocol parser
///
/// Format: `"ST,GS,  0.450kg\r\n"` (continuous push)
/// - ST = stable, US = unstable
/// - GS = gross weight, NT = net weight
pub struct CasParser;

impl ProtocolParser for CasParser {
    fn parse(&self, data: &[u8]) -> Option<ScaleReading> {
        let line = std::str::from_utf8(data).ok()?.trim();
        if line.len() < 10 {
            return None;
        }

        // Split by comma: ["ST", "GS", "  0.450kg"]
        let parts: Vec<&str> = line.splitn(3, ',').collect();
        if parts.len() < 3 {
            return None;
        }

        let stable = parts[0].trim() == "ST";
        let weight_part = parts[2].trim();

        // Extract numeric portion and unit
        let (weight_str, unit) = parse_weight_and_unit(weight_part);
        let weight = Decimal::from_str(weight_str.trim()).ok()?;

        Some(ScaleReading {
            weight,
            unit,
            stable,
            raw: line.to_string(),
        })
    }

    fn tare_command(&self) -> Vec<u8> {
        b"T\r\n".to_vec()
    }

    fn zero_command(&self) -> Vec<u8> {
        b"Z\r\n".to_vec()
    }
}

/// Toledo / MT-SICS scale protocol parser
///
/// Format: `"S S     0.450 kg\r\n"`
/// - First S = command echo
/// - Second S = stable, D = dynamic/unstable
pub struct ToledoParser;

impl ProtocolParser for ToledoParser {
    fn parse(&self, data: &[u8]) -> Option<ScaleReading> {
        let line = std::str::from_utf8(data).ok()?.trim();
        if line.len() < 8 {
            return None;
        }

        // Format: "S S     0.450 kg" or "S D     0.123 kg"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        // parts[0] = "S" (command echo)
        // parts[1] = "S"|"D" (stable/dynamic)
        // parts[2] = weight value
        // parts[3] = unit
        let stable = parts[1] == "S";
        let weight = Decimal::from_str(parts[2]).ok()?;
        let unit = match parts[3].to_lowercase().as_str() {
            "kg" => ScaleUnit::Kg,
            "g" => ScaleUnit::Gram,
            "lb" => ScaleUnit::Lb,
            _ => ScaleUnit::Kg,
        };

        Some(ScaleReading {
            weight,
            unit,
            stable,
            raw: line.to_string(),
        })
    }

    fn tare_command(&self) -> Vec<u8> {
        b"TA\r\n".to_vec()
    }

    fn zero_command(&self) -> Vec<u8> {
        b"Z\r\n".to_vec()
    }
}

/// Generic scale protocol parser
///
/// Accepts simple formats like `"0.450"` or `"0.450 kg"` per line.
/// Assumes readings are always stable.
pub struct GenericParser;

impl ProtocolParser for GenericParser {
    fn parse(&self, data: &[u8]) -> Option<ScaleReading> {
        let line = std::str::from_utf8(data).ok()?.trim();
        if line.is_empty() {
            return None;
        }

        let (weight_str, unit) = parse_weight_and_unit(line);
        let weight = Decimal::from_str(weight_str.trim()).ok()?;

        Some(ScaleReading {
            weight,
            unit,
            stable: true,
            raw: line.to_string(),
        })
    }

    fn tare_command(&self) -> Vec<u8> {
        b"T\r\n".to_vec()
    }

    fn zero_command(&self) -> Vec<u8> {
        b"Z\r\n".to_vec()
    }
}

/// Helper: split a string like "0.450kg" or "0.450 g" into (weight_str, unit)
fn parse_weight_and_unit(s: &str) -> (&str, ScaleUnit) {
    let s = s.trim();
    // Try to find where digits/decimals end
    let num_end = s
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != ' ')
        .unwrap_or(s.len());

    let weight_str = &s[..num_end];
    let unit_str = s[num_end..].trim().to_lowercase();

    let unit = match unit_str.as_str() {
        "kg" => ScaleUnit::Kg,
        "g" | "gram" => ScaleUnit::Gram,
        "lb" | "lbs" => ScaleUnit::Lb,
        _ => ScaleUnit::Kg,
    };

    (weight_str, unit)
}

/// Create a parser for the given protocol
pub fn create_parser(protocol: &ScaleProtocol) -> Box<dyn ProtocolParser> {
    match protocol {
        ScaleProtocol::Cas => Box::new(CasParser),
        ScaleProtocol::Toledo => Box::new(ToledoParser),
        ScaleProtocol::Generic => Box::new(GenericParser),
    }
}

// ---------------------------------------------------------------------------
// ScaleService (requires `hardware` feature for serial port access)
// ---------------------------------------------------------------------------

#[cfg(feature = "hardware")]
pub use service::ScaleService;

#[cfg(feature = "hardware")]
mod service {
    use super::*;
    use serialport::SerialPort;
    use std::io::BufRead;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::watch;
    use tracing::{debug, error, info, warn};

    /// Service that reads from a serial scale and publishes readings via a watch channel
    pub struct ScaleService {
        config: ScaleConfig,
        status: Arc<parking_lot::Mutex<ScaleStatus>>,
        tx: watch::Sender<ScaleReading>,
        rx: watch::Receiver<ScaleReading>,
        /// Shared handle to the serial port for sending tare/zero commands
        port_handle: Arc<parking_lot::Mutex<Option<Box<dyn SerialPort>>>>,
    }

    impl ScaleService {
        /// Create a new ScaleService with the given configuration
        pub fn new(config: ScaleConfig) -> Self {
            let (tx, rx) = watch::channel(ScaleReading::default());
            Self {
                config,
                status: Arc::new(parking_lot::Mutex::new(ScaleStatus::Disconnected)),
                tx,
                rx,
                port_handle: Arc::new(parking_lot::Mutex::new(None)),
            }
        }

        /// Start the background reader thread. Call once after construction.
        pub fn start(&self) {
            if !self.config.enabled {
                info!("Scale service disabled in config");
                return;
            }

            let config = self.config.clone();
            let tx = self.tx.clone();
            let status = Arc::clone(&self.status);
            let port_handle = Arc::clone(&self.port_handle);

            tokio::task::spawn_blocking(move || {
                let parser = create_parser(&config.protocol);

                loop {
                    // Attempt to connect
                    {
                        *status.lock() = ScaleStatus::Connecting;
                    }
                    info!("Connecting to scale on {} at {} baud ({:?} protocol)...",
                        config.port, config.baud_rate, config.protocol);

                    match serialport::new(&config.port, config.baud_rate)
                        .timeout(Duration::from_secs(5))
                        .open()
                    {
                        Ok(port) => {
                            info!("Scale connected on {}", config.port);
                            {
                                *status.lock() = ScaleStatus::Connected;
                            }

                            // Clone port for commands
                            match port.try_clone() {
                                Ok(cmd_port) => {
                                    *port_handle.lock() = Some(cmd_port);
                                }
                                Err(e) => {
                                    warn!("Could not clone serial port for commands: {}", e);
                                }
                            }

                            // Read loop
                            let mut reader = std::io::BufReader::new(port);
                            let mut line_buf = String::new();

                            loop {
                                line_buf.clear();
                                match reader.read_line(&mut line_buf) {
                                    Ok(0) => {
                                        // EOF — port disconnected
                                        warn!("Scale disconnected (EOF)");
                                        break;
                                    }
                                    Ok(_) => {
                                        if let Some(reading) = parser.parse(line_buf.as_bytes()) {
                                            debug!("Scale reading: {:.3} {} stable={}",
                                                reading.weight, reading.unit.label("en"), reading.stable);
                                            let _ = tx.send(reading);
                                        }
                                    }
                                    Err(e) => {
                                        if e.kind() == std::io::ErrorKind::TimedOut {
                                            // Timeout is normal for push-based scales between readings
                                            continue;
                                        }
                                        warn!("Scale read error: {}", e);
                                        break;
                                    }
                                }
                            }

                            // Clean up command port handle
                            *port_handle.lock() = None;
                        }
                        Err(e) => {
                            error!("Failed to open scale port {}: {}", config.port, e);
                            *status.lock() = ScaleStatus::Error(e.to_string());
                        }
                    }

                    // Reconnect after delay
                    {
                        *status.lock() = ScaleStatus::Disconnected;
                    }
                    info!("Reconnecting to scale in 3 seconds...");
                    std::thread::sleep(Duration::from_secs(3));
                }
            });
        }

        /// Subscribe to live scale readings
        pub fn subscribe(&self) -> watch::Receiver<ScaleReading> {
            self.rx.clone()
        }

        /// Send tare command to the scale
        pub fn tare(&self) {
            let parser = create_parser(&self.config.protocol);
            let cmd = parser.tare_command();
            if let Some(port) = self.port_handle.lock().as_mut() {
                if let Err(e) = std::io::Write::write_all(port, &cmd) {
                    warn!("Failed to send tare command: {}", e);
                }
            }
        }

        /// Send zero command to the scale
        pub fn zero(&self) {
            let parser = create_parser(&self.config.protocol);
            let cmd = parser.zero_command();
            if let Some(port) = self.port_handle.lock().as_mut() {
                if let Err(e) = std::io::Write::write_all(port, &cmd) {
                    warn!("Failed to send zero command: {}", e);
                }
            }
        }

        /// Get the current connection status
        pub fn status(&self) -> ScaleStatus {
            self.status.lock().clone()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    #[test]
    fn test_cas_parser_stable_reading() {
        let parser = CasParser;
        let data = b"ST,GS,  0.450kg\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("0.450").unwrap());
        assert_eq!(reading.unit, ScaleUnit::Kg);
        assert!(reading.stable);
    }

    #[test]
    fn test_cas_parser_unstable_reading() {
        let parser = CasParser;
        let data = b"US,GS,  0.123kg\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("0.123").unwrap());
        assert!(!reading.stable);
    }

    #[test]
    fn test_toledo_parser_stable() {
        let parser = ToledoParser;
        let data = b"S S     0.450 kg\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("0.450").unwrap());
        assert_eq!(reading.unit, ScaleUnit::Kg);
        assert!(reading.stable);
    }

    #[test]
    fn test_toledo_parser_dynamic() {
        let parser = ToledoParser;
        let data = b"S D     0.123 kg\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("0.123").unwrap());
        assert!(!reading.stable);
    }

    #[test]
    fn test_parser_malformed_data() {
        let cas = CasParser;
        assert!(cas.parse(b"garbage").is_none());
        assert!(cas.parse(b"").is_none());
        assert!(cas.parse(b"ST").is_none());

        let toledo = ToledoParser;
        assert!(toledo.parse(b"X").is_none());
        assert!(toledo.parse(b"").is_none());
    }

    #[test]
    fn test_parser_gram_unit() {
        let toledo = ToledoParser;
        let data = b"S S     450 g\r\n";
        let reading = toledo.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from(450));
        assert_eq!(reading.unit, ScaleUnit::Gram);
        assert!(reading.stable);
    }

    #[test]
    fn test_generic_parser() {
        let parser = GenericParser;
        let data = b"0.750 kg\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("0.750").unwrap());
        assert_eq!(reading.unit, ScaleUnit::Kg);
        assert!(reading.stable); // Generic always reports stable
    }

    #[test]
    fn test_generic_parser_number_only() {
        let parser = GenericParser;
        let data = b"1.250\r\n";
        let reading = parser.parse(data).expect("should parse");
        assert_eq!(reading.weight, Decimal::from_str("1.250").unwrap());
    }

    #[test]
    fn test_scale_protocol_from_str() {
        assert_eq!(ScaleProtocol::from_str("cas").unwrap(), ScaleProtocol::Cas);
        assert_eq!(ScaleProtocol::from_str("toledo").unwrap(), ScaleProtocol::Toledo);
        assert_eq!(ScaleProtocol::from_str("mt-sics").unwrap(), ScaleProtocol::Toledo);
        assert_eq!(ScaleProtocol::from_str("generic").unwrap(), ScaleProtocol::Generic);
    }

    #[test]
    fn test_cas_parser_net_weight() {
        let parser = CasParser;
        let data = b"ST,NT,  1.200kg\r\n";
        let reading = parser.parse(data).expect("should parse net weight");
        assert_eq!(reading.weight, Decimal::from_str("1.200").unwrap());
        assert!(reading.stable);
    }

    #[test]
    fn test_watch_channel_receives_readings() {
        let (tx, mut rx) = tokio::sync::watch::channel(ScaleReading::default());

        let reading = ScaleReading {
            weight: Decimal::from_str("0.500").unwrap(),
            unit: ScaleUnit::Kg,
            stable: true,
            raw: "test".to_string(),
        };

        tx.send(reading.clone()).unwrap();

        let received = rx.borrow_and_update();
        assert_eq!(received.weight, Decimal::from_str("0.500").unwrap());
        assert!(received.stable);
    }
}
