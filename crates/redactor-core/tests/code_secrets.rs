//! Secrets inside JavaScript served by a target. Every value is fictional.

use redactor_core::{Config, Redactor};

fn redact(text: &str) -> String {
    Redactor::new(&Config::default()).redact(text).output
}

#[test]
fn hides_keys_and_signing_material_in_minified_js() {
    let secrets = [
        "ZmFrZVNlY3JldEZvclRlc3RzMTIzNDU2Nzg=",
        "918273645",
        "s3cr3t-signing-key-0042",
        "Zx81kLmQ09pPq7Rt",
        "AKfake0123456789xyz",
        "dGVzdC10b2tlbi1mYWtlLTk5OQ==",
        "Qk9yN2xLbUV4YW1wbGVLZXk5",
    ];
    let js = format!(
        concat!(
            r#"const f=Xa.sha256.hmac.create(q1.Base64.decode("{0}"));f.update(s);"#,
            r#"const m={{"X-Ca-Key":"{1}","X-Ca-Timestamp":o.getTime(),"X-Ca-Stage":"TEST"}};"#,
            "\n",
            r#"const sig=crypto.createHmac("sha256","{2}").update(body).digest("hex");"#,
            "\n",
            r#"const appSecret="{3}";let apiKey='{4}';var t=atob("{5}");"#,
            "\n",
            r#"window.__CFG__={{region:"eu",build:"{6}"}};"#,
        ),
        secrets[0], secrets[1], secrets[2], secrets[3], secrets[4], secrets[5], secrets[6],
    );
    let out = redact(&js);
    for secret in secrets {
        assert!(!out.contains(secret), "{secret} leaked:\n{out}");
    }
    // The code around them is untouched.
    assert!(out.contains("crypto.createHmac(\"sha256\","), "{out}");
    assert!(out.contains("\"X-Ca-Stage\":\"TEST\""), "{out}");
}

#[test]
fn leaves_ordinary_code_alone() {
    let js = concat!(
        r#"import{a as handleSubmitButtonClick2}from"./chunk-vendors.d41d8cd98f00b204.js";"#,
        r#"const sortKey="createdAt",i18nKey="home.title",cls="btn btn-primary";"#,
        r#"if(mode==="webpackChunkMyApp2024Build")x=JSON.parse(raw);"#,
        r#"const sri="sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K";"#,
    );
    assert_eq!(redact(js), js);
}
