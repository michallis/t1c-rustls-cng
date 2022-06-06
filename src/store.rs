use std::ptr;

use widestring::U16CString;
use windows::{
    core::PCSTR,
    Win32::Security::Cryptography::{
        CertCloseStore, CertDuplicateCertificateContext, CertFindCertificateInStore, CertOpenStore,
        PFXImportCertStore, CERT_FIND_FLAGS, CERT_FIND_ISSUER_STR, CERT_FIND_SUBJECT_STR,
        CERT_OPEN_STORE_FLAGS, CERT_QUERY_ENCODING_TYPE, CERT_STORE_OPEN_EXISTING_FLAG,
        CERT_SYSTEM_STORE_CURRENT_SERVICE_ID, CERT_SYSTEM_STORE_CURRENT_USER_ID,
        CERT_SYSTEM_STORE_LOCAL_MACHINE_ID, CERT_SYSTEM_STORE_LOCATION_SHIFT, CRYPTOAPI_BLOB,
        CRYPT_KEY_FLAGS, HCERTSTORE, HCRYPTPROV_LEGACY, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
    },
};

use crate::{cert::CertContext, error::CngError};

const MY_ENCODING_TYPE: CERT_QUERY_ENCODING_TYPE =
    CERT_QUERY_ENCODING_TYPE(PKCS_7_ASN_ENCODING.0 | X509_ASN_ENCODING.0);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum CertStoreType {
    LocalMachine,
    CurrentUser,
    CurrentService,
}

impl CertStoreType {
    fn as_flags(&self) -> u32 {
        match self {
            CertStoreType::LocalMachine => {
                CERT_SYSTEM_STORE_LOCAL_MACHINE_ID << CERT_SYSTEM_STORE_LOCATION_SHIFT
            }
            CertStoreType::CurrentUser => {
                CERT_SYSTEM_STORE_CURRENT_USER_ID << CERT_SYSTEM_STORE_LOCATION_SHIFT
            }
            CertStoreType::CurrentService => {
                CERT_SYSTEM_STORE_CURRENT_SERVICE_ID << CERT_SYSTEM_STORE_LOCATION_SHIFT
            }
        }
    }
}

pub struct CertStore(HCERTSTORE);

unsafe impl Send for CertStore {}
unsafe impl Sync for CertStore {}

impl CertStore {
    pub fn inner(&self) -> HCERTSTORE {
        self.0
    }

    pub fn open(store_type: CertStoreType, store_name: &str) -> Result<CertStore, CngError> {
        unsafe {
            let store_name = U16CString::from_str_unchecked(store_name);
            let handle = CertOpenStore(
                PCSTR(10 as _),
                CERT_QUERY_ENCODING_TYPE::default(),
                HCRYPTPROV_LEGACY::default(),
                CERT_OPEN_STORE_FLAGS(store_type.as_flags() | CERT_STORE_OPEN_EXISTING_FLAG.0),
                store_name.as_ptr() as _,
            )?;
            Ok(CertStore(handle))
        }
    }

    pub fn from_pkcs12(data: &[u8], password: &str) -> Result<CertStore, CngError> {
        unsafe {
            let blob = CRYPTOAPI_BLOB {
                cbData: data.len() as u32,
                pbData: data.as_ptr() as _,
            };

            let store = PFXImportCertStore(&blob, password, CRYPT_KEY_FLAGS::default())?;
            Ok(CertStore(store))
        }
    }

    pub fn find_by_subject_str<S>(&self, subject: S) -> Result<Vec<CertContext>, CngError>
    where
        S: AsRef<str>,
    {
        self.find_by_str(subject.as_ref(), CERT_FIND_SUBJECT_STR)
    }

    pub fn find_by_issuer_str<S>(&self, subject: S) -> Result<Vec<CertContext>, CngError>
    where
        S: AsRef<str>,
    {
        self.find_by_str(subject.as_ref(), CERT_FIND_ISSUER_STR)
    }

    fn find_by_str(
        &self,
        subject: &str,
        flags: CERT_FIND_FLAGS,
    ) -> Result<Vec<CertContext>, CngError> {
        let mut certs = Vec::new();
        let subject = unsafe { U16CString::from_str_unchecked(subject) };

        let mut cert = ptr::null();

        loop {
            cert = unsafe {
                CertFindCertificateInStore(
                    self.0,
                    MY_ENCODING_TYPE.0,
                    0,
                    flags,
                    subject.as_ptr() as _,
                    cert,
                )
            };
            if cert.is_null() {
                break;
            } else {
                // increase refcount because it will be released by next call to CertFindCertificateInStore
                let cert = unsafe { CertDuplicateCertificateContext(cert) };
                certs.push(CertContext::owned(cert))
            }
        }
        Ok(certs)
    }
}

impl Drop for CertStore {
    fn drop(&mut self) {
        unsafe { CertCloseStore(self.0, 0) };
    }
}
