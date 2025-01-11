//! This module contains utility functions for working with [Multiaddr].

use libp2p::multiaddr::Protocol;
use libp2p::{Multiaddr, PeerId};

/// An extention trait that adds utility functions to [Multiaddr].
pub trait MultiaddrExt {
    /// Returns if this address is a public address that can be used as a relay
    /// server or a bootstrap.
    fn is_public(&self) -> bool;

    /// Returns the peer id of this address.
    fn peer_id(&self) -> Option<PeerId>;
}

impl MultiaddrExt for Multiaddr {
    fn is_public(&self) -> bool {
        let mut protocols = self.iter();
        match protocols.next() {
            Some(Protocol::Ip4(address)) => {
                if address.is_broadcast()
                    || address.is_documentation()
                    || address.is_link_local()
                    || address.is_loopback()
                    || address.is_multicast()
                    || address.is_private()
                    || address.is_unspecified()
                {
                    return false;
                }
            }
            Some(Protocol::Dns4(_)) => {}
            _ => return false,
        };
        if !matches!(protocols.next(), Some(Protocol::Udp(_))) {
            return false;
        }
        if !matches!(protocols.next(), Some(Protocol::QuicV1)) {
            return false;
        }
        if !matches!(protocols.next(), Some(Protocol::P2p(_))) {
            return false;
        }
        protocols.next().is_none()
    }

    fn peer_id(&self) -> Option<PeerId> {
        match self.iter().last() {
            Some(Protocol::P2p(peer_id)) => Some(peer_id),
            _ => None,
        }
    }
}
