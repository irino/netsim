use holo_yang::{load_module, new_context};
use yang3::context::Context;

/// Metadata for a pinned IETF/IANA YANG module used in schema validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedModule {
    pub name: &'static str,
    pub revision: &'static str,
    pub source_url: &'static str,
    pub description: &'static str,
}

/// Upstream pinned YANG module metadata and revisions.
pub const PINNED_MODULES: &[PinnedModule] = &[
    PinnedModule {
        name: "iana-if-type",
        revision: "2017-01-19",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/iana-if-type@2017-01-19.yang",
        description: "IANA Interface Type definitions (RFC 7224)",
    },
    PinnedModule {
        name: "iana-routing-types",
        revision: "2018-10-29",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/iana-routing-types@2018-10-29.yang",
        description: "IANA Routing Type definitions (RFC 8294)",
    },
    PinnedModule {
        name: "ietf-interfaces",
        revision: "2018-02-20",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-interfaces@2018-02-20.yang",
        description: "A YANG Data Model for Interface Management (RFC 8343)",
    },
    PinnedModule {
        name: "ietf-ip",
        revision: "2018-02-22",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-ip@2018-02-22.yang",
        description: "A YANG Data Model for IP Management (RFC 8344)",
    },
    PinnedModule {
        name: "ietf-routing",
        revision: "2018-03-13",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-routing@2018-03-13.yang",
        description: "A YANG Data Model for Routing Management (RFC 8349 NMDA)",
    },
    PinnedModule {
        name: "ietf-ipv4-unicast-routing",
        revision: "2018-03-13",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-ipv4-unicast-routing@2018-03-13.yang",
        description: "IPv4 Unicast Routing Management (RFC 8349 NMDA)",
    },
    PinnedModule {
        name: "ietf-ospf",
        revision: "2022-10-19",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-ospf@2022-10-19.yang",
        description: "YANG Data Model for the OSPF Protocol (RFC 9129)",
    },
    PinnedModule {
        name: "ietf-bgp",
        revision: "2023-07-05",
        source_url: "https://raw.githubusercontent.com/holo-routing/holo/master/holo-yang/modules/ietf/ietf-bgp@2023-07-05.yang",
        description: "YANG Data Model for Border Gateway Protocol (BGP-4)",
    },
];

/// Creates an initialized libyang Context with all relevant pinned IETF modules loaded.
pub fn create_yang_context() -> Context {
    let mut ctx = new_context();
    load_module(&mut ctx, "iana-if-type");
    load_module(&mut ctx, "iana-routing-types");
    load_module(&mut ctx, "ietf-interfaces");
    load_module(&mut ctx, "ietf-ip");
    load_module(&mut ctx, "ietf-routing");
    load_module(&mut ctx, "ietf-ipv4-unicast-routing");
    load_module(&mut ctx, "ietf-ospf");
    load_module(&mut ctx, "ietf-bgp");
    ctx
}
