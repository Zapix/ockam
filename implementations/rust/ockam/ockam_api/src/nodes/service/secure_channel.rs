use super::map_multiaddr_err;
use crate::error::ApiError;
use crate::nodes::models::secure_channel::{
    CreateSecureChannelListenerRequest, CreateSecureChannelRequest, CreateSecureChannelResponse,
    DeleteSecureChannelRequest, DeleteSecureChannelResponse, ShowSecureChannelRequest,
    ShowSecureChannelResponse,
};
use crate::nodes::NodeManager;
use minicbor::Decoder;
use ockam::identity::TrustEveryonePolicy;
use ockam::{Address, Result, Route};
use ockam_core::api::{Request, Response, ResponseBuilder};
use ockam_core::AsyncTryClone;
use ockam_identity::{IdentityIdentifier, TrustMultiIdentifiersPolicy};
use ockam_multiaddr::MultiAddr;

impl NodeManager {
    pub(super) async fn create_secure_channel_impl<'a>(
        &mut self,
        sc_route: Route,
        authorized_identifiers: Option<Vec<IdentityIdentifier>>,
    ) -> Result<Address> {
        let identity = self.identity()?.async_try_clone().await?;

        // If channel was already created, do nothing.
        if let Some(channel) = self.registry.secure_channels.get_by_route(&sc_route) {
            let addr = channel.addr();
            trace!(%addr, "Using cached secure channel");
            return Ok(addr.clone());
        }

        // Else, create it.
        trace!(%sc_route, "Creating secure channel");
        let sc_addr = match authorized_identifiers.clone() {
            Some(ids) => {
                identity
                    .create_secure_channel(
                        sc_route.clone(),
                        TrustMultiIdentifiersPolicy::new(ids),
                        &self.authenticated_storage,
                    )
                    .await
            }
            None => {
                identity
                    .create_secure_channel(
                        sc_route.clone(),
                        TrustEveryonePolicy,
                        &self.authenticated_storage,
                    )
                    .await
            }
        }?;

        trace!(%sc_route, %sc_addr, "Created secure channel");

        self.registry
            .secure_channels
            .insert(sc_addr.clone(), sc_route, authorized_identifiers);

        // Return secure channel address
        Ok(sc_addr)
    }

    pub(super) async fn create_secure_channel<'a>(
        &mut self,
        req: &Request<'_>,
        dec: &mut Decoder<'_>,
    ) -> Result<ResponseBuilder<CreateSecureChannelResponse<'a>>> {
        let CreateSecureChannelRequest {
            addr,
            authorized_identifiers,
            ..
        } = dec.decode()?;

        info!("Handling request to create a new secure channel: {}", addr);

        let authorized_identifiers = match authorized_identifiers {
            Some(ids) => {
                let ids = ids
                    .into_iter()
                    .map(|x| IdentityIdentifier::try_from(x.0.as_ref()))
                    .collect::<Result<Vec<IdentityIdentifier>>>()?;

                Some(ids)
            }
            None => None,
        };

        // TODO: Improve error handling + move logic into CreateSecureChannelRequest
        let addr = MultiAddr::try_from(addr.as_ref()).map_err(map_multiaddr_err)?;
        let route = crate::multiaddr_to_route(&addr)
            .ok_or_else(|| ApiError::generic("Invalid Multiaddr"))?;

        let channel = self
            .create_secure_channel_impl(route, authorized_identifiers)
            .await?;

        let response = Response::ok(req.id()).body(CreateSecureChannelResponse::new(&channel));

        Ok(response)
    }

    pub(super) async fn delete_secure_channel<'a>(
        &mut self,
        req: &Request<'_>,
        dec: &mut Decoder<'_>,
    ) -> Result<ResponseBuilder<DeleteSecureChannelResponse<'a>>> {
        let body: DeleteSecureChannelRequest = dec.decode()?;

        info!(
            "Handling request to delete secure channel: {}",
            body.channel
        );

        let identity = self.identity()?;

        let sc_address = Address::from(body.channel.as_ref());

        debug!(%sc_address, "Deleting secure channel");

        let res = match identity.stop_secure_channel(&sc_address).await {
            Ok(()) => {
                trace!(%sc_address, "Removed secure channel");
                self.registry.secure_channels.remove_by_addr(&sc_address);
                Some(sc_address)
            }
            Err(err) => {
                trace!(%sc_address, "Error removing secure channel: {err}");
                None
            }
        };

        Ok(Response::ok(req.id()).body(DeleteSecureChannelResponse::new(res)))
    }

    pub(super) fn list_secure_channels(
        &mut self,
        req: &Request<'_>,
    ) -> ResponseBuilder<Vec<String>> {
        Response::ok(req.id()).body(
            self.registry
                .secure_channels
                .list()
                .iter()
                .map(|v| v.addr().to_string())
                .collect(),
        )
    }

    pub(super) async fn show_secure_channel<'a>(
        &mut self,
        req: &Request<'_>,
        dec: &mut Decoder<'_>,
    ) -> Result<ResponseBuilder<ShowSecureChannelResponse<'a>>> {
        let body: ShowSecureChannelRequest = dec.decode()?;

        let sc_address = Address::from(body.channel.as_ref());

        debug!(%sc_address, "On show secure channel");

        let info = self.registry.secure_channels.get_by_addr(&sc_address);

        Ok(Response::ok(req.id()).body(ShowSecureChannelResponse::new(info)))
    }

    pub(super) async fn create_secure_channel_listener_impl(
        &mut self,
        addr: Address,
        authorized_identifiers: Option<Vec<IdentityIdentifier>>,
    ) -> Result<()> {
        info!(
            "Handling request to create a new secure channel listener: {}",
            addr
        );

        let identity = self.identity()?;

        match authorized_identifiers {
            Some(ids) => {
                identity
                    .create_secure_channel_listener(
                        addr.clone(),
                        TrustMultiIdentifiersPolicy::new(ids),
                        &self.authenticated_storage,
                    )
                    .await
            }
            None => {
                identity
                    .create_secure_channel_listener(
                        addr.clone(),
                        TrustEveryonePolicy,
                        &self.authenticated_storage,
                    )
                    .await
            }
        }?;

        self.registry
            .secure_channel_listeners
            .insert(addr, Default::default());

        Ok(())
    }

    pub(super) async fn create_secure_channel_listener(
        &mut self,
        req: &Request<'_>,
        dec: &mut Decoder<'_>,
    ) -> Result<ResponseBuilder<()>> {
        let CreateSecureChannelListenerRequest {
            addr,
            authorized_identifiers,
            ..
        } = dec.decode()?;

        let authorized_identifiers = match authorized_identifiers {
            Some(ids) => {
                let ids = ids
                    .into_iter()
                    .map(|x| IdentityIdentifier::try_from(x.0.as_ref()))
                    .collect::<Result<Vec<IdentityIdentifier>>>()?;

                Some(ids)
            }
            None => None,
        };

        let addr = Address::from(addr.as_ref());
        if !addr.is_local() {
            return Ok(Response::bad_request(req.id()));
        }

        self.create_secure_channel_listener_impl(addr, authorized_identifiers)
            .await?;

        let response = Response::ok(req.id());

        Ok(response)
    }

    pub(super) fn list_secure_channel_listener(
        &mut self,
        req: &Request<'_>,
    ) -> ResponseBuilder<Vec<String>> {
        Response::ok(req.id()).body(
            self.registry
                .secure_channel_listeners
                .iter()
                .map(|(addr, _)| addr.to_string())
                .collect(),
        )
    }
}
