export { createRelayServer } from "./server/index.js";
export type { RelayServerOptions, RelayServer } from "./server/index.js";

export {
  encodeBase64,
  decodeBase64,
  serializeEnvelope,
  deserializeEnvelope,
  parseClientMessage,
  stringifyServerMessage,
} from "./protocol/index.js";

export type {
  SerializedEnvelope,
  ClientMessage,
  ServerMessage,
} from "./protocol/index.js";

export {
  handleWsOpen,
  handleWsMessage,
  handleWsClose,
} from "./transport/index.js";

export type { WsConnection } from "./transport/index.js";
