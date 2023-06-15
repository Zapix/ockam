use clap::{command, Args};
use colorful::Colorful;
use miette::IntoDiagnostic;
use ockam::{Context, TcpTransport};
use ockam_api::nodes::models::services::StartKafkaOutletRequest;
use ockam_api::nodes::models::services::StartServiceRequest;
use ockam_core::api::Request;

use tokio::{sync::Mutex, try_join};

use crate::node::get_node_name;
use crate::{
    fmt_log, fmt_ok,
    kafka::{kafka_default_outlet_addr, kafka_default_outlet_server},
    node::NodeOpts,
    service::start::start_service_impl,
    terminal::OckamColor,
    util::node_rpc,
    CommandGlobalOpts,
};

/// Create a new Kafka Outlet
#[derive(Clone, Debug, Args)]
pub struct CreateCommand {
    #[command(flatten)]
    node_opts: NodeOpts,
    /// The local address of the service
    #[arg(long, default_value_t = kafka_default_outlet_addr())]
    addr: String,
    /// The address of the kafka bootstrap broker
    #[arg(long, default_value_t = kafka_default_outlet_server())]
    bootstrap_server: String,
}

impl CreateCommand {
    pub fn run(self, options: CommandGlobalOpts) {
        node_rpc(rpc, (options, self));
    }
}

async fn rpc(ctx: Context, (opts, cmd): (CommandGlobalOpts, CreateCommand)) -> miette::Result<()> {
    opts.terminal
        .write_line(&fmt_log!("Creating KafkaOutlet service"))?;
    let CreateCommand {
        node_opts,
        addr,
        bootstrap_server,
    } = cmd;
    let is_finished = Mutex::new(false);
    let send_req = async {
        let tcp = TcpTransport::create(&ctx).await.into_diagnostic()?;

        let payload = StartKafkaOutletRequest::new(bootstrap_server.clone());
        let payload = StartServiceRequest::new(payload, &addr);
        let req = Request::post("/node/services/kafka_outlet").body(payload);
        let node_name = get_node_name(&opts.state, &node_opts.at_node);

        start_service_impl(&ctx, &opts, &node_name, "KafkaOutlet", req, Some(&tcp)).await?;
        *is_finished.lock().await = true;

        Ok::<_, crate::Error>(())
    };

    let msgs = vec![
        format!(
            "Building KafkaOutlet service {}",
            &addr.to_string().color(OckamColor::PrimaryResource.color())
        ),
        format!(
            "Starting KafkaOutlet service, connecting to {}",
            &bootstrap_server
                .to_string()
                .color(OckamColor::PrimaryResource.color())
        ),
    ];
    let progress_output = opts.terminal.progress_output(&msgs, &is_finished);
    let (_, _) = try_join!(send_req, progress_output)?;

    opts.terminal
        .stdout()
        .plain(fmt_ok!(
            "KafkaOutlet service started at {}\n",
            &bootstrap_server
                .to_string()
                .color(OckamColor::PrimaryResource.color())
        ))
        .write_line()?;

    Ok(())
}
