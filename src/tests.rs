use crate::pomerium;

#[test]
fn simple_conf() {
    const SIMPLE_CONF: &str = "
    routes:
    - from: https://somedomain.com
      policy:
      - allow:
          or:
          - email:
              is: myemail@place.com
      to: http://127.0.0.1:8123
";
    println!("{:#?}", pomerium::load_from_str(SIMPLE_CONF));
}

#[test]
fn no_policy_restricts_user() {
    const SIMPLE_CONF: &str = "
    routes:
    - from: https://somedomain.com
      policy: []
      to: http://127.0.0.1:8123
";
    assert_eq!(
        pomerium::load_from_str(SIMPLE_CONF).routes[0].policy.check_authorized("myemail@place.com"), 
        false
    );
}