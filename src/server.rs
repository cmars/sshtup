use std;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

extern crate error_chain;
use error_chain::ChainedError;

extern crate futures;
use futures::{Async, Future, Poll};

extern crate rustupolis;
use rustupolis::space::{Match, Space};
use rustupolis::store::SimpleStore;
use rustupolis::tuple::Tuple;

extern crate thrussh;
use thrussh::*;
use thrussh::server::{Auth, Session};

extern crate thrussh_keys;
use thrussh_keys::key;

use ast::Statement;
use error::{Result, ResultExt};
use grammar;

pub type SharedSpace = Arc<Mutex<Space<SimpleStore>>>;

pub enum HState {
    Query(Match, Session, ChannelId),
    Out(
        Box<Future<Item = (), Error = rustupolis::error::Error>>,
        Session,
        ChannelId,
    ),
    Finished(Session),
}

pub struct H {
    current_line: Vec<u8>,
    space: SharedSpace,
    state: Option<HState>,
}

impl H {
    pub fn new() -> H {
        H {
            current_line: vec![],
            space: Arc::new(Mutex::new(Space::new(SimpleStore::new()))),
            state: None,
        }
    }
    fn with_state(mut self, state: HState) -> Self {
        self.state = Some(state);
        self
    }
    fn readline(&mut self) -> Result<Option<Statement>> {
        if self.current_line.is_empty() {
            return Ok(None);
        }
        let raw_line = self.current_line.drain(..).collect::<Vec<u8>>();
        let line = std::str::from_utf8(&raw_line).chain_err(|| "invalid utf8")?;
        let stmt = grammar::statement(line).chain_err(|| "parse error")?;
        Ok(Some(stmt))
    }
    fn write_tup(session: &mut Session, channel: ChannelId, tup: Tuple) {
        let msg = format!("{:?}", tup);
        H::write_string(session, channel, &msg);
    }
    fn write_string(session: &mut Session, channel: ChannelId, msg: &str) {
        session.data(channel, None, msg.as_bytes());
    }
    fn write_error(session: &mut Session, channel: ChannelId, e: rustupolis::error::Error) {
        let msg = format!("{}", e.display_chain());
        H::write_string(session, channel, &msg);
    }
    fn write_prompt(session: &mut Session, channel: ChannelId) {
        H::write_string(session, channel, "\r\n> ");
    }
}

pub struct HFuture(Option<H>);

impl HFuture {
    fn new(h: H) -> HFuture {
        HFuture(Some(h))
    }
}

impl Future for HFuture {
    type Item = (H, thrussh::server::Session);
    type Error = ();
    fn poll(&mut self) -> Poll<(H, thrussh::server::Session), ()> {
        let mut h = match self.0.take() {
            Some(h) => h,
            None => return Err(()),
        };
        let session = match h.state.take() {
            Some(HState::Query(mut f, mut session, channel)) => {
                match f.poll() {
                    Ok(Async::Ready(Some(tup))) => H::write_tup(&mut session, channel, tup),
                    Ok(Async::Ready(None)) => H::write_string(&mut session, channel, "none"),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => H::write_error(&mut session, channel, e),
                }
                H::write_prompt(&mut session, channel);
                session
            }
            Some(HState::Out(mut f, mut session, channel)) => {
                match f.poll() {
                    Ok(Async::Ready(())) => H::write_string(&mut session, channel, "ok"),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => H::write_error(&mut session, channel, e),
                }
                H::write_prompt(&mut session, channel);
                session
            }
            Some(HState::Finished(session)) => session,
            None => return Err(()),
        };
        Ok(Async::Ready((h, session)))
    }
}

impl server::Server for H {
    type Handler = Self;
    fn new(&self, _: SocketAddr) -> Self {
        H {
            current_line: vec![],
            space: self.space.clone(),
            state: None,
        }
    }
}

impl server::Handler for H {
    type Error = ();
    type FutureAuth = futures::Finished<(Self, server::Auth), Self::Error>;
    type FutureUnit = HFuture;
    type FutureBool = futures::Finished<(Self, server::Session, bool), Self::Error>;

    fn finished_auth(self, auth: Auth) -> Self::FutureAuth {
        futures::finished((self, auth))
    }
    fn finished_bool(self, session: Session, b: bool) -> Self::FutureBool {
        futures::finished((self, session, b))
    }
    fn finished(self, session: Session) -> Self::FutureUnit {
        HFuture::new(self.with_state(HState::Finished(session)))
    }
    fn auth_publickey(self, user: &str, key: &key::PublicKey) -> Self::FutureAuth {
        println!("connection from {:?} public key {:?}", user, key);
        futures::finished((self, server::Auth::Accept))
    }
    fn channel_open_session(self, channel: ChannelId, mut session: Session) -> Self::FutureUnit {
        // banner
        session.data(channel, None, b"sshtupd - welcome to tuplespace\r\n");
        H::write_prompt(&mut session, channel);
        HFuture::new(self.with_state(HState::Finished(session)))
    }
    fn channel_close(self, _channel: ChannelId, session: Session) -> Self::FutureUnit {
        HFuture::new(self.with_state(HState::Finished(session)))
    }
    fn channel_eof(self, _channel: ChannelId, session: Session) -> Self::FutureUnit {
        HFuture::new(self.with_state(HState::Finished(session)))
    }
    fn data(
        mut self,
        channel: ChannelId,
        data: &[u8],
        mut session: server::Session,
    ) -> Self::FutureUnit {
        // Block on current space operation
        match self.state.take() {
            None => {}
            Some(HState::Finished(_)) => {}
            Some(state) => return HFuture::new(self.with_state(state)),
        };
        match data {
            b"\x04" => {
                session.eof(channel);
                session.close(channel);
                return HFuture::new(self.with_state(HState::Finished(session)));
            }
            b"\r" => {
                // line break
                session.data(channel, None, b"\r\n");
                let new_state = match self.readline() {
                    Ok(Some(Statement::In(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        HState::Query(space.in_(tup), session, channel)
                    }
                    Ok(Some(Statement::Rd(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        HState::Query(space.rd(tup), session, channel)
                    }
                    Ok(Some(Statement::Out(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        HState::Out(space.out(tup), session, channel)
                    }
                    Ok(None) => {
                        H::write_prompt(&mut session, channel);
                        HState::Finished(session)
                    }
                    Err(e) => {
                        let msg = format!("{}", e.display_chain());
                        H::write_string(&mut session, channel, &msg);
                        H::write_prompt(&mut session, channel);
                        HState::Finished(session)
                    }
                };
                HFuture::new(self.with_state(new_state))
            }
            b => {
                self.current_line.push(b[0]);
                session.data(channel, None, data);
                HFuture::new(self.with_state(HState::Finished(session)))
            }
        }
    }
}
