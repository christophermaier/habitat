fn main() {
    println!("IP Address from custom function");
    let ip = habitat_core::util::sys::ip();
    println!(">>>>>>> ip = {:?}", ip);
    println!();
    println!("Hostname from custom function");
    let hostname = habitat_core::os::net::hostname();
    println!(">>>>>>> hostname = {:?}", hostname);
    println!();
    println!("IP address from dns_lookup crate: lookup_host");
    let lookup_host = dns_lookup::lookup_host(&dns_lookup::get_hostname().unwrap());
    println!(">>>>>>> lookup_host = {:?}", lookup_host);
    println!();
    println!("IP address from dns_lookup crate: getaddrinfo");
    let getaddrinfo_result = getaddrinfo();
    println!(">>>>>>> getaddrinfo_result = {:?}", getaddrinfo_result);
    println!();
    println!("Hostname from dns_lookup crate");
    let dns_hostname = dns_lookup::get_hostname();
    println!(">>>>>>> dns_hostname = {:?}", dns_hostname);
    println!();
    println!("IP addrs from get_ip_addrs crate");
    let ips = get_if_addrs::get_if_addrs().unwrap();
    for ip in ips {
        if !ip.is_loopback() {
            if let get_if_addrs::IfAddr::V4(ref ifaddr) = ip.addr {
                println!(">>>>>>> ip = {:?}", ip);
                println!(">>>>>>> ifaddr = {:?}", ifaddr);
                println!(">>>>>>> ip address = {:?}", ifaddr.ip);
            }
        }
    }
}

fn getaddrinfo() -> std::io::Result<Vec<dns_lookup::AddrInfo>> {
    let hostname = dns_lookup::get_hostname().expect("no hostname?!");
    let hints = dns_lookup::AddrInfoHints { protocol: libc::AF_INET,
                                            ..dns_lookup::AddrInfoHints::default() };
    dns_lookup::getaddrinfo(Some(&hostname), None, Some(hints))?
        .collect::<std::io::Result<Vec<dns_lookup::AddrInfo>>>()
}
