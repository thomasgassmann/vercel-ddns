use eyre::{eyre, Result};
use futures::executor;
use log::{error, info};
use public_ip::{dns, BoxToResolver, ToResolver};
use structopt::clap::{arg_enum, AppSettings};
use structopt::StructOpt;

use crate::vercel::{add_dns_record, Record};

fn get_public_ips(ip_types: &Vec<IpType>) -> Result<Vec<(IpType, String)>> {
    let mut res: Vec<(IpType, String)> = Vec::new();
    for ip_type in ip_types.iter() {
        let resolver = match *ip_type {
            IpType::IPV4 => vec![
                BoxToResolver::new(dns::OPENDNS_RESOLVER_V4),
                BoxToResolver::new(dns::GOOGLE_DNS_TXT_RESOLVER_V4),
            ],
            IpType::IPV6 => vec![
                BoxToResolver::new(dns::OPENDNS_RESOLVER_V6),
                BoxToResolver::new(dns::GOOGLE_DNS_TXT_RESOLVER_V6),
            ],
        };

        match executor::block_on(public_ip::resolve_address(resolver.to_resolver())) {
            Some(ip) => res.push((ip_type.clone(), ip.to_string())),
            None => return Err(eyre!("Unable to get public IP.")),
        }
    }

    return Ok(res);
}

arg_enum! {
    #[derive(Debug,PartialEq,Clone)]
    pub enum IpType {
        IPV4,
        IPV6,
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    about,
    setting(AppSettings::ColoredHelp),
    setting(AppSettings::ColorAuto)
)]
pub struct Args {
    #[structopt(short, long, env = "VDDNS_DOMAIN")]
    pub domain: String,

    #[structopt(short, long, env = "VDDNS_SUBDOMAIN")]
    pub subdomain: Vec<String>,

    #[structopt(short, long, possible_values = &IpType::variants(), case_insensitive = true, default_value = "ipv4", env = "VDDNS_IP_TYPE")]
    pub ip_type: Vec<IpType>,

    #[structopt(long, default_value = "3600", env = "VDDNS_TTL")]
    pub ttl: i64,

    #[structopt(short, long, env = "VERCEL_TOKEN")]
    pub token: String,
}

pub fn run(args: Args) -> Result<()> {
    let ips = match get_public_ips(&args.ip_type) {
        Ok(ip) => ip,
        Err(e) => {
            error!("Unable to get public ip. {}", e.to_string());
            return Ok(());
        }
    };

    for subdomain in args.subdomain.iter() {
        for (ip_type, ip) in ips.iter() {
            let rec = Record::new(
                subdomain.to_string(),
                ip.to_string(),
                match *ip_type {
                    IpType::IPV4 => String::from("A"),
                    IpType::IPV6 => String::from("AAAA"),
                },
                args.ttl,
            );
        
            match add_dns_record(&args.domain, &args.token, rec) {
                Ok(_) => {
                    info!("Record added / updated sucessfully");
                }
                Err(e) => {
                    error!("Unable to add / update the record. {}", e.to_string());
                    return Ok(());
                }
            }
        }
    }

    return Ok(());
}
