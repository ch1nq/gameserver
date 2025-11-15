mod tests {

    #[test]
    fn test_key_id_format() {
        // Use a valid RSA private key for testing
        let test_pem = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC2RrLNE/QKgneY
QpyNcFuEkIpdMWOHMPXAbPZc0ypBY1COCU7Dx3rVT0Sn7UsZE/fwYImxTMUtp6sz
5MTPr6QpmwZbAJyYUbId2SbxT2jORKYSdtqc1aySAdrUdsQxaB/xhmIwkWRk6ZTI
tw6Uf6lktaLBS2QL3/+z55k1iMs+w+FlKu1TfLArPT6UllzWzOgSvaxOTnWw5IPl
c77MiDm+YF3eO9FKHkC4l2ftZEEM2lXxuFwFrHqNm7BKjuwkzWHm1ARLghBH0KQZ
N8p3ysExS1dyziOJKBdAZNplaK9zGRJLaUU71nNNQKjFbwtMd/KqER9RTZYfKEMJ
pveg4OYPAgMBAAECggEAHNcj3Fn/X5hUFvXXMnPoLxn1opg5cL8Y60jyVC6fPXha
2xZy7XxHHbAso0ti+gVUUibcMn78peQlLRFR6LCYT3L1dvmqTVmDzsA4rq7LXPO0
uTAwF+ehJfsAJmTiVxTsFPmX2KpwkZz5yyZXurxWT5aDuYTVwCFBorQO5E8QJY5w
/D/7qvdkMgmdyXjW+d6eApBmj8Wue/hq3QXCVVsTgA/FDVPPUfH52vx/O8ABhT5+
VtTRZqiQYCkuVrGIJ0qStp/W99XOeHAn02/UIoMh1a4G2LkZY+VP8wttE6KrZ1VW
hBTbvBWwMqAPP7gIYecScbPjXclW3GbmtzaASmr4lQKBgQDw3Y5lmPoxpAmHaGbA
n/IZRTTh1qMXWX1+s+FXhfsuGEdrt48aUfEPs3erIcSXD/ExCx8pDq8tB6GQe+ZO
bKUsONh+f0gZxM+37V9K/bvp0MtGAXzcDuvcBPB79N+8F9pwdZNa2UG44kEMgzyd
E1mzReCe0+Phywb0XHAyP6gM7QKBgQDBurQfFAndoJLHuTyMQsOnVcBcKH1bQ5fI
Y5xq+dX9NyTUjEsCWOiG/wRzuc4378B05L4zSUymBgTTj+fO6gVTYvFTBePrH+da
ERFmyv2Dpyj+YKRpm8TFYFQvdQv3vQoTWgqz3Q8ZPGsqdA8y1pcfcEc8107zmPQD
wjrxcxCbawKBgQCDs/HX1dUAbbyUIN8Gdq7PaIso7c8RxmobbMpLrEQTCU2MNbt2
3dVdC3nkxjsTirEMaxNnxNK+YYzTTxw4R6ntS0v9pyVKidY2sQHJJIKqr/NmXQvj
2/jVvpGshdIMrFJR6chgBamtKXH+IIh1Lw5+Ozg+QIg7f2NXHHBw2WPPZQKBgDR1
K+Tmdi1vF4/BVuXcBkK/c5EA3cDisqzuXCKTeCBS2EQ9oOoHzR8Q2tHDVFXNM93z
OpWEmZ6zLodjBi//KmYD+riydZ7rSqgWyxF8kd0eXHlVDfAS39taVDFtjkoNBDdt
QEyn5Ti+JX6fYqYveUhoDMIqwxQvLJP/+hn7QFn1AoGBAOcyh1axbKVGvQfN5LUL
Ub7SGmN8Bo8nweJQwVN++HkuJgA1qeFSAmHkTb5SWvlLo5SGnCggJOBHS2YdsWBI
6kQxb6WosnoGl3DIp3QlWTJ0KTc5zgH5ufDzUsjCf6Kixm46T00gNXxAL4394uB2
hgvjlUMEsLIcj8xxegi/k4iQ
-----END PRIVATE KEY-----"#;

        let key_id = registry_auth::key_id_from_pem(test_pem).expect("Failed to generate key ID");

        // Verify the format: 12 groups of 4 characters separated by colons
        let parts: Vec<&str> = key_id.split(':').collect();
        assert_eq!(
            parts.len(),
            12,
            "Key ID should have 12 colon-separated groups"
        );

        for (i, part) in parts.iter().enumerate() {
            assert_eq!(
                part.len(),
                4,
                "Group {} should have 4 characters, got: {}",
                i,
                part
            );

            // Verify all characters are valid base32 (A-Z, 2-7)
            for ch in part.chars() {
                assert!(
                    ch.is_ascii_uppercase() || ('2'..='7').contains(&ch),
                    "Invalid base32 character: {}",
                    ch
                );
            }
        }

        println!("Generated key ID: {}", key_id);
    }
}
