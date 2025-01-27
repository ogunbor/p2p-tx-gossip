use libp2p::{
    identify::{Identify, IdentifyConfig},
    mdns::{Mdns, MdnsConfig, MdnsEvent},
    noise::{Keypair, NoiseConfig, X25519Spec},
    swarm::SwarmBuilder,
    tcp, yamux, Multiaddr, MultiaddrExt, NetworkBehaviour, PeerId, Swarm, Transport,
};
use std::error::Error;
use tokio::io::{self, AsyncBufReadExt};

#[derive(NetworkBehaviour)]
struct Behaviour {
    mdns: Mdns,
    identify: Identify,
}

impl Behaviour {
    fn new(local_keypair: libp2p::identity::Keypair) -> Result<Self, Box<dyn Error>> {
        let local_peer_id = PeerId::from(local_keypair.public());
        let mdns = Mdns::new(MdnsConfig::default())?;
        let identify = Identify::new(IdentifyConfig::new(
            "/my-p2p/1.0".into(),
            local_keypair.public(),
        ));
        Ok(Self { mdns, identify })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_keypair = libp2p::identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_keypair.public());

    println!("Local peer id: {local_peer_id}");

    let noise_keys = Keypair::<X25519Spec>::new().into_authentic(&local_keypair)?;
    let transport = tcp::tokio::Transport::new(tcp::Config::default())
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(noise_keys).into_authenticated())
        .multiplex(yamux::Config::default())
        .boxed();

    let behaviour = Behaviour::new(local_keypair)?;
    let mut swarm = SwarmBuilder::new(transport, behaviour, local_peer_id.clone())
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    // Start listening on a random local port
    let listen_addr: Multiaddr = "/ip4/0.0.0.0/tcp/0".parse()?;
    swarm.listen_on(listen_addr)?;

    // Reading user input to allow connecting to other nodes
    let mut stdin = io::BufReader::new(io::stdin()).lines();

    tokio::spawn(async move {
        while let Some(Ok(line)) = stdin.next_line().await {
            if let Ok(addr) = line.parse::<Multiaddr>() {
                swarm.dial(addr).unwrap();
            } else {
                println!("Invalid multiaddr.");
            }
        }
    });

    loop {
        tokio::select! {
            event = swarm.select_next_some() => match event {
                libp2p::swarm::SwarmEvent::Behaviour(MdnsEvent::Discovered(peers)) => {
                    for (peer_id, _) in peers {
                        println!("Discovered peer: {}", peer_id);
                    }
                },
                libp2p::swarm::SwarmEvent::Behaviour(MdnsEvent::Expired(peers)) => {
                    for (peer_id, _) in peers {
                        println!("Lost peer: {}", peer_id);
                    }
                },
                libp2p::swarm::SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Listening on {}", address);
                },
                other => {
                    println!("Unhandled event: {:?}", other);
                }
            }
        }
    }
}
