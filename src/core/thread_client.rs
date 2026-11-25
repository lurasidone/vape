use super::Client;

// The native mixin build can't add a field to Horizon's Game type. Keep the
// client's runtime in thread-local state and expose a cloned handle to hooks.
thread_local! {
    static CLIENT: Client = Client::default();
}

pub fn client() -> Client {
    CLIENT.with(::std::clone::Clone::clone)
}
