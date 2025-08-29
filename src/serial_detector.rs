use anyhow::Result;
use log::info;
use serialport::{SerialPortInfo, SerialPortType};
use std::path::PathBuf;

// Known Meshtastic USB vendor/product IDs
const MESHTASTIC_VENDOR_IDS: &[(u16, u16)] = &[
    (0x239a, 0x4000), // RAK4631 (Adafruit)
    (0x239a, 0x8029), // RAK4631 (Adafruit) alternate
    (0x303a, 0x1001), // ESP32-S3
    (0x10c4, 0xea60), // CP210x UART Bridge (used by many boards)
    (0x0403, 0x6001), // FTDI FT232
    (0x0403, 0x6015), // FTDI FT-X series
    (0x1a86, 0x55d4), // CH9102 (used by many boards)
    (0x2e8a, 0x000a), // Raspberry Pi Pico
];

// Known device descriptions that indicate Meshtastic hardware
const MESHTASTIC_DESCRIPTIONS: &[&str] = &[
    "RAK4631",
    "LILYGO",
    "T-Beam",
    "T-Echo",
    "Heltec",
    "Nano G1",
    "Station G1",
    "CP210",     // Silicon Labs CP210x (common on ESP32 boards)
    "CH910",     // CH9102 (common on ESP32 boards)
    "FT232",     // FTDI chips
    "Meshtastic",
    "WisBlock",
];

pub async fn detect_meshtastic_port() -> Result<PathBuf> {
    info!("Auto-detecting Meshtastic serial port...");
    
    let ports = serialport::available_ports()
        .map_err(|e| anyhow::anyhow!("Failed to list serial ports: {}", e))?;
    
    if ports.is_empty() {
        return Err(anyhow::anyhow!("No serial ports found"));
    }
    
    info!("Found {} serial port(s) to check", ports.len());
    
    // First, try to find ports that match known Meshtastic devices
    let mut meshtastic_ports = Vec::new();
    let mut possible_ports = Vec::new();
    
    for port_info in &ports {
        let port_name = &port_info.port_name;
        
        // Skip obvious non-candidates
        if port_name.contains("Bluetooth") {
            continue;
        }
        
        // Check if this is a known Meshtastic device
        if is_likely_meshtastic(&port_info) {
            info!("Found likely Meshtastic device: {} - {}", 
                  port_name, 
                  get_port_description(&port_info));
            meshtastic_ports.push(port_name.clone());
        } else if is_possible_meshtastic(&port_info) {
            info!("Found possible Meshtastic device: {} - {}", 
                  port_name,
                  get_port_description(&port_info));
            possible_ports.push(port_name.clone());
        }
    }
    
    // Try likely ports first
    for port in meshtastic_ports {
        info!("Checking likely Meshtastic port: {}", port);
        if verify_meshtastic_port(&port).await? {
            return Ok(PathBuf::from(port));
        }
    }
    
    // Then try possible ports
    for port in possible_ports {
        info!("Checking possible port: {}", port);
        if verify_meshtastic_port(&port).await? {
            return Ok(PathBuf::from(port));
        }
    }
    
    // If nothing worked, show what we found
    info!("No Meshtastic devices detected. Available ports:");
    for port_info in &ports {
        info!("  {} - {}", port_info.port_name, get_port_description(&port_info));
    }
    
    Err(anyhow::anyhow!(
        "No Meshtastic devices found. Please check:\n\
        - Device is connected via USB\n\
        - Device is powered on\n\
        - No other apps are using the device\n\
        Use --serial-port to specify manually"
    ))
}

fn is_likely_meshtastic(port_info: &SerialPortInfo) -> bool {
    match &port_info.port_type {
        SerialPortType::UsbPort(usb_info) => {
            // Check vendor/product ID
            for &(vid, pid) in MESHTASTIC_VENDOR_IDS {
                if usb_info.vid == vid && usb_info.pid == pid {
                    return true;
                }
            }
            
            // Check manufacturer
            if let Some(manufacturer) = &usb_info.manufacturer {
                let manufacturer_lower = manufacturer.to_lowercase();
                if manufacturer_lower.contains("meshtastic") ||
                   manufacturer_lower.contains("rak") ||
                   manufacturer_lower.contains("lilygo") ||
                   manufacturer_lower.contains("heltec") {
                    return true;
                }
            }
            
            // Check product name
            if let Some(product) = &usb_info.product {
                for keyword in MESHTASTIC_DESCRIPTIONS {
                    if product.contains(keyword) {
                        return true;
                    }
                }
            }
            
            // Check serial number patterns
            if let Some(serial) = &usb_info.serial_number {
                if serial.starts_with("M") || serial.contains("mesh") {
                    return true;
                }
            }
            
            false
        }
        _ => false,
    }
}

fn is_possible_meshtastic(port_info: &SerialPortInfo) -> bool {
    match &port_info.port_type {
        SerialPortType::UsbPort(usb_info) => {
            // Common USB-to-serial chips that might be Meshtastic
            let common_chips = [
                (0x10c4, 0xea60), // CP210x
                (0x1a86, 0x7523), // CH340
                (0x1a86, 0x55d4), // CH9102
                (0x0403, 0x6001), // FTDI
                (0x0403, 0x6015), // FTDI FT-X
            ];
            
            for &(vid, pid) in &common_chips {
                if usb_info.vid == vid && usb_info.pid == pid {
                    return true;
                }
            }
            
            // Check for ESP32 indicators
            if let Some(product) = &usb_info.product {
                let product_lower = product.to_lowercase();
                if product_lower.contains("esp32") || 
                   product_lower.contains("usb") ||
                   product_lower.contains("uart") {
                    return true;
                }
            }
            
            false
        }
        _ => false,
    }
}

fn get_port_description(port_info: &SerialPortInfo) -> String {
    match &port_info.port_type {
        SerialPortType::UsbPort(usb_info) => {
            let manufacturer = usb_info.manufacturer.as_deref().unwrap_or("Unknown");
            let product = usb_info.product.as_deref().unwrap_or("Unknown");
            format!("{} - {} (VID:{:04X} PID:{:04X})", 
                    manufacturer, product, usb_info.vid, usb_info.pid)
        }
        _ => "Unknown device".to_string(),
    }
}

async fn verify_meshtastic_port(port: &str) -> Result<bool> {
    use meshtastic::api::StreamApi;
    use meshtastic::utils;
    use std::time::Duration;
    use tokio::time::timeout;
    
    let stream_api = StreamApi::new();
    
    // Try to build a serial stream with standard Meshtastic settings
    let serial_stream = match utils::stream::build_serial_stream(
        port.to_string(),
        Some(115200),  // Standard Meshtastic baud rate
        Some(true),    // DTR
        Some(true),    // RTS
    ) {
        Ok(stream) => stream,
        Err(e) => {
            info!("Failed to open {}: {}", port, e);
            return Ok(false);
        }
    };
    
    // Try to connect with a timeout
    let connect_result = timeout(
        Duration::from_secs(3),
        stream_api.connect(serial_stream)
    ).await;
    
    match connect_result {
        Ok((mut decoded_listener, _)) => {
            // Wait for a packet to verify it's a Meshtastic device
            let packet_result = timeout(
                Duration::from_secs(2),
                decoded_listener.recv()
            ).await;
            
            match packet_result {
                Ok(Some(from_radio)) => {
                    // Check if this looks like a Meshtastic packet
                    match from_radio.payload_variant {
                        Some(meshtastic::protobufs::from_radio::PayloadVariant::MyInfo(_)) |
                        Some(meshtastic::protobufs::from_radio::PayloadVariant::NodeInfo(_)) |
                        Some(meshtastic::protobufs::from_radio::PayloadVariant::Config(_)) |
                        Some(meshtastic::protobufs::from_radio::PayloadVariant::Packet(_)) => {
                            info!("Verified Meshtastic device on {}", port);
                            Ok(true)
                        }
                        _ => {
                            info!("Non-Meshtastic response on {}", port);
                            Ok(false)
                        }
                    }
                }
                _ => {
                    info!("No valid response from {}", port);
                    Ok(false)
                }
            }
        }
        Err(_) => {
            info!("Connection timeout on {}", port);
            Ok(false)
        }
    }
}