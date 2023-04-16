use openssl::{
    bn::{BigNum, MsbOption},
    x509::{
        X509, X509Name, X509Ref,
        extension::{SubjectAlternativeName, BasicConstraints},
    },
    nid::Nid,
    asn1::{Asn1Time, Asn1Integer},
    rsa::Rsa,
    pkey::{PKeyRef, PKey, Private},
    error::ErrorStack,
    hash::MessageDigest,
};


pub enum IdentityType<'a> {
    Ca,
    Entity(&'a Identity),
}

#[derive(Debug, Clone)]
pub struct Identity {
    certificate: X509,
    private_key: PKey<Private>,
}

impl Identity {
    pub fn certificate_authority() -> Result<Self, ()> {
        Ok(Self::build("bridgeburner", IdentityType::Ca).map_err(|_err| ())?)
    }

    pub fn entity_certificate(ca: &Self, dns: &str) -> Result<Self, ()> {
        Ok(Self::build(dns, IdentityType::Entity(&ca)).map_err(|_err| ())?)
    }

    fn build(name: &str, identity_type: IdentityType) -> Result<Self, ErrorStack> {
        let private_key = PKey::from_rsa(Rsa::generate(2048)?)?;
        let mut serial = BigNum::new()?;
        serial.rand(32, MsbOption::MAYBE_ZERO, false)?;

        let mut subject = X509Name::builder()?;
        subject.append_entry_by_nid(Nid::COMMONNAME, name)?;
        let subject = subject.build();

        let mut certificate = X509::builder()?;
        certificate.set_version(2)?;
        certificate.set_serial_number(Asn1Integer::from_bn(serial.as_ref())?.as_ref())?;
        certificate.set_subject_name(&subject)?;
        certificate.set_not_before(Asn1Time::days_from_now(0)?.as_ref())?;
        certificate.set_not_after(Asn1Time::days_from_now(2048)?.as_ref())?;
        certificate.set_pubkey(private_key.as_ref())?;

        match identity_type {
            IdentityType::Ca => {
                certificate.set_issuer_name(&subject)?;
                certificate.append_extension(
                        BasicConstraints::new().critical().ca().build()?
                    )?;
                certificate.sign(
                        private_key.as_ref(),
                        MessageDigest::from_nid(Nid::SHA256).unwrap()
                    )?;
            }
            IdentityType::Entity(ref ca_identity) => {
                certificate.set_issuer_name(ca_identity.certificate.subject_name())?;
                certificate.append_extension(
                        BasicConstraints::new().critical().build()?
                    )?;
                certificate.sign(
                        ca_identity.private_key.as_ref(),
                        MessageDigest::from_nid(Nid::SHA256).unwrap()
                    )?;
                certificate.append_extension(
                        SubjectAlternativeName::new()
                            .dns(name)
                            .build(&certificate.x509v3_context(
                                    Some(ca_identity.certificate.as_ref()),
                                    None
                                ))?
                    )?;
            }
        }

        Ok(Self {
            certificate: certificate.build(),
            private_key: private_key,
        })
    }

    pub fn cert_ref(&self) -> &X509Ref {
        self.certificate.as_ref()
    }

    pub fn key_ref(&self) -> &PKeyRef<Private> {
        self.private_key.as_ref()
    }
}
