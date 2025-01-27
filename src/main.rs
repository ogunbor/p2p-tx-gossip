use libp2p::{
    core::upgrade,
    gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, IdentTopic, MessageAuthenticity},
    identity,
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::{Swarm, SwarmBuilder, SwarmEvent},
    tcp::TokioTcpConfig,
    tokio,
    yamux::YamuxConfig,
    Multiaddr, NetworkBehaviour, PeerId, Transport,
};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Generate an identity keypair for the node
    let id_keys = identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(id_keys.public());
    println!("Local peer id: {:?}", peer_id);

    // Create Gossipsub configuration
    let gossipsub_config = GossipsubConfig::default();
    let mut gossipsub = Gossipsub::new(
        MessageAuthenticity::Signed(id_keys.clone()),
        gossipsub_config,
    )?;

    // Subscribe to the "transaction" topic
    let topic = IdentTopic::new("transaction");
    gossipsub.subscribe(&topic)?;

    // Create an mDNS service for local peer discovery
    let mdns = Mdns::new(MdnsConfig::default()).await?;

    // Combine behaviours
    #[derive(NetworkBehaviour)]
    struct MyBehaviour {
        gossipsub: Gossipsub,
        mdns: Mdns,
    }

    let behaviour = MyBehaviour { gossipsub, mdns };

    // Set up the transport
    let noise_keys = Keypair::<X25519Spec>::new().into_authentic(&id_keys)?;
    let transport = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(YamuxConfig::default())
        .boxed();

    // Create the swarm
    let mut swarm = SwarmBuilder::new(transport, behaviour, peer_id)
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    // Listen on a random port
    let addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse()?;
    Swarm::listen_on(&mut swarm, addr)?;

    // Event loop
    loop {
        match swarm.select_next_some().await {
            SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(GossipsubEvent::Message {
                propagation_source,
                message_id,
                message,
            })) => {
                println!(
                    "Received message: {:?} with ID: {:?} from peer: {:?}",
                    String::from_utf8_lossy(&message.data),
                    message_id,
                    propagation_source
                );
            }
            SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(MdnsEvent::Discovered(peers))) => {
                for (peer, _addr) in peers {
                    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer);
                }
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {:?}", address);
            }
            _ => {}
        }
    }
}
