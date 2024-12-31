macro_rules! define_flags {
    ($($name:ident,)*) => {
        $(
            pub const $name: &'static str = stringify!($name);
        )*

    };
    () => {};
}

define_flags! {
    FLAGS_AUTO_EXPAND,
    FLAGS_FORK,
    // FLAGS_N_EVENT,
}
