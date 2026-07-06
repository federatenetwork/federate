//! Minimal DNS wire format: enough to parse a question and build A/AAAA
//! answers or SERVFAIL. Anything else is forwarded upstream as raw bytes.

use std::net::IpAddr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    A,
    Aaaa,
    Other(u16),
}

impl QueryType {
    fn from_u16(v: u16) -> Self {
        match v {
            1 => QueryType::A,
            28 => QueryType::Aaaa,
            other => QueryType::Other(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DnsQuery {
    pub id: u16,
    pub name: String,
    pub qtype: QueryType,
    /// Raw question section bytes (name + qtype + qclass) for echoing back.
    pub question_bytes: Vec<u8>,
    pub recursion_desired: bool,
}

#[derive(Debug)]
pub struct ParseError(pub &'static str);

impl DnsQuery {
    pub fn parse(packet: &[u8]) -> Result<Self, ParseError> {
        if packet.len() < 12 {
            return Err(ParseError("packet too short"));
        }
        let id = u16::from_be_bytes([packet[0], packet[1]]);
        let flags = u16::from_be_bytes([packet[2], packet[3]]);
        if flags & 0x8000 != 0 {
            return Err(ParseError("not a query"));
        }
        let qdcount = u16::from_be_bytes([packet[4], packet[5]]);
        if qdcount == 0 {
            return Err(ParseError("no question"));
        }
        // Parse first question name (no compression in questions).
        let mut pos = 12;
        let mut labels = Vec::new();
        loop {
            let len = *packet.get(pos).ok_or(ParseError("truncated name"))? as usize;
            pos += 1;
            if len == 0 {
                break;
            }
            if len & 0xC0 != 0 {
                return Err(ParseError("compressed question name"));
            }
            let label = packet
                .get(pos..pos + len)
                .ok_or(ParseError("truncated label"))?;
            labels.push(String::from_utf8_lossy(label).to_string());
            pos += len;
        }
        let qtype_bytes = packet
            .get(pos..pos + 4)
            .ok_or(ParseError("truncated qtype/qclass"))?;
        let qtype = u16::from_be_bytes([qtype_bytes[0], qtype_bytes[1]]);
        let question_bytes = packet[12..pos + 4].to_vec();
        Ok(Self {
            id,
            name: labels.join("."),
            qtype: QueryType::from_u16(qtype),
            question_bytes,
            recursion_desired: flags & 0x0100 != 0,
        })
    }
}

fn header(query: &DnsQuery, rcode: u8, ancount: u16) -> Vec<u8> {
    let mut flags: u16 = 0x8000; // QR = response
    flags |= 0x0400; // AA = authoritative for Federate names
    if query.recursion_desired {
        flags |= 0x0100; // RD echoed
    }
    flags |= 0x0080; // RA
    flags |= rcode as u16 & 0x000F;
    let mut out = Vec::with_capacity(12);
    out.extend(query.id.to_be_bytes());
    out.extend(flags.to_be_bytes());
    out.extend(1u16.to_be_bytes()); // QDCOUNT
    out.extend(ancount.to_be_bytes());
    out.extend(0u16.to_be_bytes()); // NSCOUNT
    out.extend(0u16.to_be_bytes()); // ARCOUNT
    out
}

/// Build a response with one A/AAAA record per IP, pointing the answers back
/// at the question name via a compression pointer.
pub fn build_response(_packet: &[u8], query: &DnsQuery, ips: &[IpAddr], ttl: u32) -> Vec<u8> {
    let mut out = header(query, 0, ips.len() as u16);
    out.extend(&query.question_bytes);
    for ip in ips {
        out.extend([0xC0, 0x0C]); // pointer to name at offset 12
        match ip {
            IpAddr::V4(v4) => {
                out.extend(1u16.to_be_bytes()); // TYPE A
                out.extend(1u16.to_be_bytes()); // CLASS IN
                out.extend(ttl.to_be_bytes());
                out.extend(4u16.to_be_bytes());
                out.extend(v4.octets());
            }
            IpAddr::V6(v6) => {
                out.extend(28u16.to_be_bytes()); // TYPE AAAA
                out.extend(1u16.to_be_bytes());
                out.extend(ttl.to_be_bytes());
                out.extend(16u16.to_be_bytes());
                out.extend(v6.octets());
            }
        }
    }
    out
}

pub fn build_servfail(_packet: &[u8], query: &DnsQuery) -> Vec<u8> {
    let mut out = header(query, 2, 0);
    out.extend(&query.question_bytes);
    out
}

/// Build a raw query packet (used by `federate dns test`).
pub fn build_query(id: u16, name: &str, qtype: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(id.to_be_bytes());
    out.extend(0x0100u16.to_be_bytes()); // RD
    out.extend(1u16.to_be_bytes()); // QDCOUNT
    out.extend([0u8; 6]); // AN/NS/AR
    for label in name.trim_end_matches('.').split('.') {
        out.push(label.len() as u8);
        out.extend(label.as_bytes());
    }
    out.push(0);
    out.extend(qtype.to_be_bytes());
    out.extend(1u16.to_be_bytes()); // CLASS IN
    out
}

/// Parse A/AAAA answers out of a response packet (skips other record types).
pub fn parse_answers(packet: &[u8]) -> Result<(u8, Vec<(IpAddr, u32)>), ParseError> {
    if packet.len() < 12 {
        return Err(ParseError("short response"));
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let rcode = (flags & 0x000F) as u8;
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]);
    let ancount = u16::from_be_bytes([packet[6], packet[7]]);
    let mut pos = 12;
    let skip_name = |packet: &[u8], mut pos: usize| -> Result<usize, ParseError> {
        loop {
            let len = *packet.get(pos).ok_or(ParseError("truncated"))? as usize;
            if len & 0xC0 == 0xC0 {
                return Ok(pos + 2);
            }
            pos += 1;
            if len == 0 {
                return Ok(pos);
            }
            pos += len;
        }
    };
    for _ in 0..qdcount {
        pos = skip_name(packet, pos)? + 4;
    }
    let mut answers = Vec::new();
    for _ in 0..ancount {
        pos = skip_name(packet, pos)?;
        let fixed = packet
            .get(pos..pos + 10)
            .ok_or(ParseError("truncated answer"))?;
        let rtype = u16::from_be_bytes([fixed[0], fixed[1]]);
        let ttl = u32::from_be_bytes([fixed[4], fixed[5], fixed[6], fixed[7]]);
        let rdlen = u16::from_be_bytes([fixed[8], fixed[9]]) as usize;
        pos += 10;
        let rdata = packet
            .get(pos..pos + rdlen)
            .ok_or(ParseError("truncated rdata"))?;
        pos += rdlen;
        match (rtype, rdlen) {
            (1, 4) => answers.push((IpAddr::from([rdata[0], rdata[1], rdata[2], rdata[3]]), ttl)),
            (28, 16) => {
                let mut o = [0u8; 16];
                o.copy_from_slice(rdata);
                answers.push((IpAddr::from(o), ttl));
            }
            _ => {}
        }
    }
    Ok((rcode, answers))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_response_roundtrip_multiple_gateways() {
        let packet = build_query(0x1234, "home.fed", 1);
        let query = DnsQuery::parse(&packet).unwrap();
        assert_eq!(query.name, "home.fed");
        assert_eq!(query.qtype, QueryType::A);

        let ips: Vec<IpAddr> = vec![
            "45.1.1.1".parse().unwrap(),
            "45.2.2.2".parse().unwrap(),
            "45.3.3.3".parse().unwrap(),
        ];
        let response = build_response(&packet, &query, &ips, 30);
        let (rcode, answers) = parse_answers(&response).unwrap();
        assert_eq!(rcode, 0);
        assert_eq!(answers.len(), 3);
        assert!(answers.iter().all(|(_, ttl)| *ttl == 30));
        assert_eq!(answers[0].0, ips[0]);

        let servfail = build_servfail(&packet, &query);
        let (rcode, answers) = parse_answers(&servfail).unwrap();
        assert_eq!(rcode, 2);
        assert!(answers.is_empty());
    }
}
