use axum::{routing::get, Router, response::IntoResponse};
use regex::Regex;
use serde_json::{json, Value};
use std::env;
use std::fs::{self, File, read_to_string};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use tokio::time::{sleep, Duration};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use rand::{thread_rng, Rng};

async fn hello_world() -> &'static str {
    "Hello, world!"
}

async fn setup_environment() {
    let mut rng = rand::thread_rng();
    let random_port: u16 = rng.gen_range(20000..=60000); 
    let argo_domain = std::env::var("ARGO_DOMAIN").unwrap_or_else(|_| "default_value".to_string());
    let argo_auth = std::env::var("ARGO_AUTH").unwrap_or_else(|_| "default_value".to_string());

    let env_vars = [
        ("UUID", "66e5c8dd-3176-458e-8fb0-1ed91d2f9602"),
        // ("NEZHA_SERVER", "nz.abc.com"),
        // ("NEZHA_PORT", "5555"),
        // ("NEZHA_KEY", ""),
        ("ARGO_DOMAIN", argo_domain.as_str()),  // argo固定隧道也可在scrects中添加环境变量
        ("ARGO_AUTH", argo_auth.as_str()),    // argo密钥，留空将使用临时隧道
        ("CFIP", "www.visa.com.tw"),
        ("CFPORT", "443"),
        ("NAME", "shuttle"),
        ("FILE_PATH", "./tmp"),
        ("ARGO_PORT", "8080"), // argo端口,
        ("SUB_PATH", "sub"), // 订阅路径
    ];

    for (key, default_value) in env_vars {
        if env::var(key).is_err() {
            env::set_var(key, default_value);
        }
    }
}

async fn read_sub() -> impl IntoResponse {
    let file_path = env::var("FILE_PATH").unwrap_or_else(|_| "./tmp".to_string());
    let sub_path = env::var("SUB_PATH").unwrap_or_else(|_| "sub".to_string()); 
    match read_to_string(format!("{}/{}.txt", file_path, sub_path)) { 
        Ok(content) => content,
        Err(_) => "Failed to read sub.txt".to_string(),
    }
}

async fn create_config_files() {
    let file_path = env::var("FILE_PATH").unwrap_or_else(|_| "./tmp".to_string());
    let uuid = env::var("UUID").unwrap_or_default();
    let argo_port = env::var("ARGO_PORT").unwrap_or_else(|_| "8080".to_string()); 
    let argo_auth = env::var("ARGO_AUTH").unwrap_or_default();
    let argo_domain = env::var("ARGO_DOMAIN").unwrap_or_default();
    
    if !Path::new(&file_path).exists() {
        fs::create_dir_all(&file_path).expect("Failed to create directory");
    }

    let old_files = ["boot.log", "sub.txt", "config.json", "tunnel.json", "tunnel.yml"];
    for file in old_files.iter() {
        let file_path = format!("{}/{}", file_path, file);
        let _ = fs::remove_file(file_path);
    }

    if !argo_auth.is_empty() && !argo_domain.is_empty() {
        if argo_auth.contains("TunnelSecret") {
            fs::write(format!("{}/tunnel.json", file_path), &argo_auth)
                .expect("Failed to write tunnel.json");

            let tunnel_id = {
                let re = Regex::new(r#""TunnelID":"([^"]+)""#).unwrap();
                re.captures(&argo_auth)
                    .and_then(|cap| cap.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default()
            };

            let tunnel_yml = format!(
                r#"tunnel: {}
credentials-file: {}/tunnel.json
protocol: http2

ingress:
  - hostname: {}
    service: http://localhost:{}
    originRequest:
      noTLSVerify: true
  - service: http_status:404
"#,
                tunnel_id, file_path, argo_domain, argo_port
            );

            fs::write(format!("{}/tunnel.yml", file_path), tunnel_yml)
                .expect("Failed to write tunnel.yml");
        }
    }
    
    let config = json!({
        "log": {
            "access": "/dev/null",
            "error": "/dev/null",
            "loglevel": "none"
        },
        "inbounds": [
            {
                "port": argo_port.parse::<i32>().unwrap_or(8080),
                "protocol": "vless",
                "settings": {
                    "clients": [
                        {
                            "id": uuid,
                            "flow": "xtls-rprx-vision"
                        }
                    ],
                    "decryption": "none",
                    "fallbacks": [
                        { "dest": 3001 },
                        { "path": "/vless-argo", "dest": 3002 },
                        { "path": "/vmess-argo", "dest": 3003 },
                        { "path": "/trojan-argo", "dest": 3004 }
                    ]
                },
                "streamSettings": {
                    "network": "tcp"
                }
            },
            {
                "port": 3001,
                "listen": "127.0.0.1",
                "protocol": "vless",
                "settings": {
                    "clients": [{ "id": uuid }],
                    "decryption": "none"
                },
                "streamSettings": {
                    "network": "ws",
                    "security": "none"
                }
            },
            {
                "port": 3002,
                "listen": "127.0.0.1",
                "protocol": "vless",
                "settings": {
                    "clients": [{ "id": uuid, "level": 0 }],
                    "decryption": "none"
                },
                "streamSettings": {
                    "network": "ws",
                    "security": "none",
                    "wsSettings": {
                        "path": "/vless-argo"
                    }
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic"],
                    "metadataOnly": false
                }
            },
            {
                "port": 3003,
                "listen": "127.0.0.1",
                "protocol": "vmess",
                "settings": {
                    "clients": [{ "id": uuid, "alterId": 0 }]
                },
                "streamSettings": {
                    "network": "ws",
                    "wsSettings": {
                        "path": "/vmess-argo"
                    }
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic"],
                    "metadataOnly": false
                }
            },
            {
                "port": 3004,
                "listen": "127.0.0.1",
                "protocol": "trojan",
                "settings": {
                    "clients": [{ "password": uuid }]
                },
                "streamSettings": {
                    "network": "ws",
                    "security": "none",
                    "wsSettings": {
                        "path": "/trojan-argo"
                    }
                },
                "sniffing": {
                    "enabled": true,
                    "destOverride": ["http", "tls", "quic"],
                    "metadataOnly": false
                }
            }
        ],
        "dns": {
            "servers": ["https+local://8.8.8.8/dns-query"]
        },
        "outbounds": [
            {
                "protocol": "freedom",
                "tag": "direct"
            },
            {
                "protocol": "blackhole",
                "tag": "block"
            }
        ]
    });

    let config_str = serde_json::to_string_pretty(&config).unwrap();
    fs::write(format!("{}/config.json", file_path), config_str)
        .expect("Failed to write config.json");
}

async fn download_files() {
    let file_path = env::var("FILE_PATH").unwrap_or_else(|_| "./tmp".to_string());
    let arch = Command::new("uname")
        .arg("-m")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default();

    let file_info = match arch.as_str() {
        "arm" | "arm64" | "aarch64" => vec![
            ("https://amd64.ssss.nyc.mn/2go", "bot"),
            ("https://arm64.ssss.nyc.mn/web", "web"),
            ("https://arm64.ssss.nyc.mn/agent", "npm"),
        ],
        "amd64" | "x86_64" | "x86" => vec![
            ("https://amd64.ssss.nyc.mn/2go", "bot"),
            ("https://amd64.ssss.nyc.mn/web", "web"),
            ("https://amd64.ssss.nyc.mn/agent", "npm"),
        ],
        _ => vec![],
    };

    for (url, filename) in file_info {
        let filepath = format!("{}/{}", file_path, filename);
        if !Path::new(&filepath).exists() {
            Command::new("curl")
                .args(["-L", "-sS", "-o", &filepath, url])
                .status()
                .expect("Failed to download file");
            
            Command::new("chmod")
                .args(["777", &filepath])
                .status()
                .expect("Failed to set permissions");
        }
    }
}

async fn run_services() {
    let file_path = env::var("FILE_PATH").unwrap_or_else(|_| "./tmp".to_string());
    
    if Path::new(&format!("{}/npm", file_path)).exists() {
        let nezha_server = env::var("NEZHA_SERVER").unwrap_or_default();
        let nezha_port = env::var("NEZHA_PORT").unwrap_or_default();
        let nezha_key = env::var("NEZHA_KEY").unwrap_or_default();

        if !nezha_server.is_empty() && !nezha_port.is_empty() && !nezha_key.is_empty() {
            let tls_ports = ["443", "8443", "2096", "2087", "2083", "2053"];
            let nezha_tls = if tls_ports.contains(&nezha_port.as_str()) { "--tls" } else { "" };
            
            Command::new(format!("{}/npm", file_path))
                .args(["-s", &format!("{}:{}", nezha_server, nezha_port), "-p", &nezha_key])
                .arg(nezha_tls)
                .spawn()
                .expect("Failed to start npm");
        }
    }

    sleep(Duration::from_secs(2)).await;

    if Path::new(&format!("{}/web", file_path)).exists() {
        Command::new(format!("{}/web", file_path))
            .args(["-c", &format!("{}/config.json", file_path)])
            .spawn()
            .expect("Failed to start web");
    }

    sleep(Duration::from_secs(2)).await;

    if Path::new(&format!("{}/bot", file_path)).exists() {
        let argo_auth = env::var("ARGO_AUTH").unwrap_or_default();
        let argo_port = env::var("ARGO_PORT").unwrap_or_default();
        
        let boot_log_path = format!("{}/boot.log", file_path);
        let tunnel_yml_path = format!("{}/tunnel.yml", file_path);
        let url = format!("http://localhost:{}", argo_port);

        let args = if argo_auth.len() >= 120 && argo_auth.len() <= 250 {
            vec!["tunnel", "--edge-ip-version", "auto", "--no-autoupdate", 
                 "--protocol", "http2", "run", "--token", &argo_auth]
        } else if argo_auth.contains("TunnelSecret") {
            vec!["tunnel", "--edge-ip-version", "auto", 
                 "--config", &tunnel_yml_path, "run"]
        } else {
            vec!["tunnel", "--edge-ip-version", "auto", "--no-autoupdate",
                 "--protocol", "http2", "--logfile", &boot_log_path,
                 "--loglevel", "info", "--url", &url]
        };

        Command::new(format!("{}/bot", file_path))
            .args(&args)
            .spawn()
            .expect("Failed to start bot");
    }
}

async fn generate_links() {
    let file_path = env::var("FILE_PATH").unwrap_or_else(|_| "./tmp".to_string());
    sleep(Duration::from_secs(6)).await;

    let argo_auth = env::var("ARGO_AUTH").unwrap_or_default();
    let argo_domain = env::var("ARGO_DOMAIN").unwrap_or_default();
    
    let argodomain = if !argo_auth.is_empty() {
        argo_domain
    } else {
        let boot_log = fs::read_to_string(format!("{}/boot.log", file_path))
            .unwrap_or_default();
        let re = Regex::new(r"https://([^/]+)\.trycloudflare\.com").unwrap();
        re.captures(&boot_log)
            .and_then(|cap| cap.get(1))
            .map(|m| format!("{}.trycloudflare.com", m.as_str()))
            .unwrap_or_default()
    };

    println!("ArgoDomain: {}", argodomain);
    sleep(Duration::from_secs(2)).await;

    let isp = Command::new("curl")
        .args(["-s", "https://speed.cloudflare.com/meta"])
        .output()
        .ok()
        .and_then(|output| {
            let output_str = String::from_utf8_lossy(&output.stdout);
            let v: Value = serde_json::from_str(&output_str).unwrap_or(json!({}));
            Some(format!("{}-{}", 
                v["country"].as_str().unwrap_or(""),
                v["asOrganization"].as_str().unwrap_or("")
            ).replace(" ", "_"))
        })
        .unwrap_or_default();

    sleep(Duration::from_secs(2)).await;

    let uuid = env::var("UUID").unwrap_or_default();
    let cfip = env::var("CFIP").unwrap_or_default();
    let cfport = env::var("CFPORT").unwrap_or_default();
    let name = env::var("NAME").unwrap_or_default();

    let vmess_config = json!({
        "v": "2",
        "ps": format!("{}-{}", name, isp),
        "add": cfip,
        "port": cfport,
        "id": uuid,
        "aid": "0",
        "scy": "none",
        "net": "ws",
        "type": "none",
        "host": argodomain,
        "path": "/vmess-argo?ed=2048",
        "tls": "tls",
        "sni": argodomain,
        "alpn": ""
    });

    let mut list_file = File::create(format!("{}/list.txt", file_path))
        .expect("Failed to create list.txt");

    writeln!(list_file, "vless://{}@{}:{}?encryption=none&security=tls&sni={}&type=ws&host={}&path=%2Fvless-argo%3Fed%3D2048#{}-{}",
        uuid, cfip, cfport, argodomain, argodomain, name, isp).unwrap();
    
    writeln!(list_file, "\nvmess://{}", 
        BASE64_STANDARD.encode(serde_json::to_string(&vmess_config).unwrap())).unwrap();
    
    writeln!(list_file, "\ntrojan://{}@{}:{}?security=tls&sni={}&type=ws&host={}&path=%2Ftrojan-argo%3Fed%3D2048#{}-{}",
        uuid, cfip, cfport, argodomain, argodomain, name, isp).unwrap();

    let list_content = fs::read_to_string(format!("{}/list.txt", file_path))
        .expect("Failed to read list.txt");
    let sub_content = BASE64_STANDARD.encode(list_content.as_bytes());
    
    fs::write(
        format!("{}/sub.txt", file_path),
        &sub_content
    ).expect("Failed to write sub.txt");

    // 打印 sub.txt 内容
    println!("\nSub Content:");
    println!("{}", sub_content);

    for file in ["list.txt", "boot.log", "config.json", "tunnel.json", "tunnel.yml"].iter() {
        let _ = fs::remove_file(format!("{}/{}", file_path, file));
    }
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    setup_environment().await;
    create_config_files().await;
    download_files().await;
    run_services().await;
    generate_links().await;

    println!("App is running!");

    let router = Router::new()
        .route("/", get(hello_world))
        .route(&format!("/{}", env::var("SUB_PATH").unwrap_or_else(|_| "sub".to_string())), get(read_sub));

    Ok(router.into())
}
