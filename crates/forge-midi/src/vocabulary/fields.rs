pub(crate) mod input {
    pub(crate) const NOTE: &str = "note";
    pub(crate) const VELOCITY: &str = "velocity";
    pub(crate) const CHANNEL: &str = "channel";
    pub(crate) const PORT_NAME: &str = "port_name";
    pub(crate) const CONTROLLER: &str = "controller";
    pub(crate) const VALUE: &str = "value";
    pub(crate) const PROGRAM: &str = "program";
}

pub(crate) mod port {
    pub(crate) const PORT_NAME: &str = "port_name";
    pub(crate) const DIRECTION: &str = "direction";

    pub(crate) mod direction {
        pub(crate) const INPUT: &str = "input";
        pub(crate) const OUTPUT: &str = "output";
        pub(crate) const ALL: [&str; 2] = [INPUT, OUTPUT];
    }
}
