//! La firma de los candados. Cada PC genera la suya la primera vez
//! (~/.config/cerrojo/firma): si la clave fuera la misma para todos, estaría
//! publicada en el código y cualquiera podría firmar un APK que se instalara
//! encima de un candado.

use anyhow::{Context, Result};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha2::Sha256;

pub struct Keys {
    pub key: RsaPrivateKey,
    pub cert: Vec<u8>,
    pub public: Vec<u8>,
}

pub fn load() -> Result<Keys> {
    let dir = crate::catalog::config_dir().join("firma");
    let (kp, cp) = (dir.join("key.pk8"), dir.join("cert.der"));
    let (key, cert) = match (std::fs::read(&kp), std::fs::read(&cp)) {
        (Ok(k), Ok(c)) => (RsaPrivateKey::from_pkcs8_der(&k).context("clave dañada")?, c),
        _ => {
            let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048)?;
            let cert = self_signed(&key)?;
            std::fs::create_dir_all(&dir)?;
            write_private(&kp, key.to_pkcs8_der()?.as_bytes())?;
            std::fs::write(&cp, &cert)?;
            (key, cert)
        }
    };
    let public = RsaPublicKey::from(&key).to_public_key_der()?.as_bytes().to_vec();
    Ok(Keys { key, cert, public })
}

fn write_private(path: &std::path::Path, data: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        f.write_all(data)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, data)?;
        Ok(())
    }
}

// ---- Certificado X.509 autofirmado, armado a mano en DER ----

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut o = vec![tag];
    let n = content.len();
    if n < 0x80 {
        o.push(n as u8);
    } else if n < 0x100 {
        o.extend_from_slice(&[0x81, n as u8]);
    } else {
        o.extend_from_slice(&[0x82, (n >> 8) as u8, n as u8]);
    }
    o.extend_from_slice(content);
    o
}

fn self_signed(key: &RsaPrivateKey) -> Result<Vec<u8>> {
    use rand::RngCore;
    let mut serial = [0u8; 8];
    rand::rngs::OsRng.fill_bytes(&mut serial);
    serial[0] = (serial[0] & 0x7f) | 0x01;

    // sha256WithRSAEncryption
    let alg = tlv(0x30, &[tlv(0x06, &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x0b]), vec![0x05, 0x00]].concat());
    let cn = tlv(0x30, &[tlv(0x06, &[0x55, 0x04, 0x03]), tlv(0x0c, b"Cerrojo")].concat());
    let name = tlv(0x30, &tlv(0x31, &cn));
    let validity = tlv(0x30, &[tlv(0x17, b"260101000000Z"), tlv(0x18, b"20991231235959Z")].concat());
    let spki = RsaPublicKey::from(key).to_public_key_der()?.as_bytes().to_vec();
    let tbs = tlv(0x30, &[tlv(0x02, &serial), alg.clone(), name.clone(), validity, name, spki].concat());
    let sig = SigningKey::<Sha256>::new(key.clone()).sign(&tbs).to_vec();
    let bits = [&[0u8][..], &sig].concat();
    Ok(tlv(0x30, &[tbs, alg, tlv(0x03, &bits)].concat()))
}
