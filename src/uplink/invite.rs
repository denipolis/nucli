use super::wire::{self, Key};

const SCHEME: &str = "nucli-uplink://pair?";

#[derive(Debug, PartialEq)]
pub struct Invite {
    pub code: String,
    pub key: Option<Key>,
    pub addresses: Vec<String>,
}

pub fn local_addresses(port: u16) -> Vec<String> {
    let mut addresses: Vec<std::net::Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|interface| !interface.is_loopback() && !interface.is_link_local() && interface.is_oper_up())
        .filter(|interface| !virtual_bridge(&interface.name))
        .filter_map(|interface| match interface.ip() {
            std::net::IpAddr::V4(ip) => Some(ip),
            std::net::IpAddr::V6(_) => None,
        })
        .collect();
    addresses.sort_by_key(|ip| (!ip.is_private(), *ip));
    addresses.dedup();
    addresses.into_iter().map(|ip| format!("{ip}:{port}")).collect()
}

fn virtual_bridge(name: &str) -> bool {
    let name = name.to_lowercase();
    ["bridge", "vmnet", "docker", "veth", "br-", "virbr", "vboxnet", "vethernet", "lxc", "cni"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

pub fn link(code: &str, key: &Key, addresses: &[String]) -> String {
    let addresses: Vec<&str> = addresses.iter().take(4).map(String::as_str).collect();
    format!("{SCHEME}code={code}&id={}&at={}", wire::encode_key(key), addresses.join(","))
}

pub fn parse(text: &str) -> Option<Invite> {
    let query = text.trim().strip_prefix(SCHEME)?;
    let mut invite = Invite { code: String::new(), key: None, addresses: Vec::new() };
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("code", value)) => invite.code = wire::normalize_code(value)?,
            Some(("id", value)) => invite.key = wire::decode_key(value),
            Some(("at", value)) => {
                invite.addresses = value.split(',').filter(|item| !item.is_empty()).map(String::from).collect()
            }
            _ => {}
        }
    }
    (!invite.code.is_empty()).then_some(invite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_round_trip() {
        let key = wire::Identity::generate().public;
        let addresses = vec!["192.168.1.20:47100".to_string(), "[fd00::1]:47100".to_string()];
        let text = link("K7QM4XRP", &key, &addresses);
        assert_eq!(parse(&text), Some(Invite { code: "K7QM4XRP".into(), key: Some(key), addresses }));
        assert_eq!(parse("K7QM-4XRP"), None);
        assert_eq!(parse("nucli-uplink://pair?code=nope"), None);
        assert!(local_addresses(1).iter().all(|address| address.ends_with(":1")));
        assert!(virtual_bridge("bridge100") && virtual_bridge("vEthernet (WSL)") && !virtual_bridge("en0"));
    }
}
