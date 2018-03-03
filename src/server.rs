use std;
use std::fmt;
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

pub enum SpaceOp {
    Query(Match),
    Out(Box<Future<Item = (), Error = rustupolis::error::Error>>),
}

impl fmt::Debug for SpaceOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &SpaceOp::Query(_) => write!(f, "Query"),
            &SpaceOp::Out(_) => write!(f, "Out"),
        }
    }
}

pub struct H {
    current_line: Vec<u8>,
    space: SharedSpace,
    op: Option<SpaceOp>,
}

impl H {
    pub fn new() -> H {
        H {
            current_line: vec![],
            space: Arc::new(Mutex::new(Space::new(SimpleStore::new()))),
            op: None,
        }
    }
    fn with_op(mut self, op: SpaceOp) -> Self {
        debug!("with_op: {:?}", op);
        self.op = Some(op);
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

pub struct HFuture {
    h: Option<H>,
    op: Option<SpaceOp>,
    session: Option<Session>,
    channel: Option<ChannelId>,
}

impl HFuture {
    fn new(h: H, op: Option<SpaceOp>, session: Session, channel: Option<ChannelId>) -> HFuture {
        HFuture {
            h: Some(h),
            op: op,
            session: Some(session),
            channel: channel,
        }
    }
}

impl Future for HFuture {
    type Item = (H, thrussh::server::Session);
    type Error = ();
    fn poll(&mut self) -> Poll<(H, thrussh::server::Session), ()> {
        let h = self.h.take().unwrap();
        let mut session = self.session.take().unwrap();
        match self.op.take() {
            Some(SpaceOp::Query(mut f)) => {
                let channel = match self.channel.take() {
                    Some(channel) => channel,
                    None => {
                        error!("missing channel to write response");
                        return Err(());
                    }
                };
                match f.poll() {
                    Ok(Async::Ready(Some(tup))) => H::write_tup(&mut session, channel, tup),
                    Ok(Async::Ready(None)) => H::write_string(&mut session, channel, "none"),
                    Ok(Async::NotReady) => {
                        return Ok(Async::Ready((h.with_op(SpaceOp::Query(f)), session)));
                    }
                    Err(e) => H::write_error(&mut session, channel, e),
                }
                H::write_prompt(&mut session, channel);
            }
            Some(SpaceOp::Out(mut f)) => {
                let channel = match self.channel.take() {
                    Some(channel) => channel,
                    None => {
                        error!("missing channel to write response");
                        return Err(());
                    }
                };
                match f.poll() {
                    Ok(Async::Ready(())) => H::write_string(&mut session, channel, "ok"),
                    Ok(Async::NotReady) => {
                        return Ok(Async::Ready((h.with_op(SpaceOp::Out(f)), session)))
                    }
                    Err(e) => H::write_error(&mut session, channel, e),
                }
                H::write_prompt(&mut session, channel);
            }
            _ => {}
        }
        Ok(Async::Ready((h, session)))
    }
}

impl server::Server for H {
    type Handler = Self;
    fn new(&self, _: SocketAddr) -> Self {
        H {
            current_line: vec![],
            space: self.space.clone(),
            op: None,
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
    fn finished(mut self, session: Session) -> Self::FutureUnit {
        let op = self.op.take();
        HFuture::new(self, op, session, None)
    }
    fn auth_publickey(self, user: &str, key: &key::PublicKey) -> Self::FutureAuth {
        println!("connection from {:?} public key {:?}", user, key);
        futures::finished((self, server::Auth::Accept))
    }
    fn channel_open_session(self, channel: ChannelId, mut session: Session) -> Self::FutureUnit {
        // banner
        session.data(channel, None, b"sshtupd: welcome to tuplespace\r\n");
        H::write_prompt(&mut session, channel);
        self.finished(session)
    }
    fn data(
        mut self,
        channel: ChannelId,
        data: &[u8],
        mut session: server::Session,
    ) -> Self::FutureUnit {
        // Block on current space operation
        if let Some(op) = self.op.take() {
            return HFuture::new(self, Some(op), session, Some(channel));
        }
        debug!("data: {:?}", data);
        match data {
            b"\x04" => {
                session.eof(channel);
                session.close(channel);
                return self.finished(session);
            }
            b"\r" => {
                // line break
                session.data(channel, None, b"\r\n");
                let new_op = match self.readline() {
                    Ok(Some(Statement::In(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        SpaceOp::Query(space.in_(tup))
                    }
                    Ok(Some(Statement::Rd(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        SpaceOp::Query(space.rd(tup))
                    }
                    Ok(Some(Statement::Out(tup))) => {
                        let mut space = self.space.lock().unwrap();
                        SpaceOp::Out(space.out(tup))
                    }
                    Ok(None) => {
                        H::write_prompt(&mut session, channel);
                        return self.finished(session);
                    }
                    Err(e) => {
                        let msg = format!("{}", e.display_chain());
                        H::write_string(&mut session, channel, &msg);
                        H::write_prompt(&mut session, channel);
                        return self.finished(session);
                    }
                };
                HFuture::new(self, Some(new_op), session, Some(channel))
            }
            b => {
                self.current_line.push(b[0]);
                session.data(channel, None, data);
                self.finished(session)
            }
        }
    }
}
