/**
 * Password Authentication module — traditional username/password login.
 *
 * Wraps `createPasswordAuthManager` from the trust layer. Optional, opt-in
 * module: relays can be built with escrow only, password only, both, or
 * neither. Login routes (in @prism/relay) issue Prism capability tokens
 * when the tokens module is also installed.
 */

import type { RelayModule, RelayContext } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import { createPasswordAuthManager } from "@prism/core/trust";
import type { PasswordAuthManagerOptions } from "@prism/core/trust";

export function passwordAuthModule(
  options: PasswordAuthManagerOptions = {},
): RelayModule {
  return {
    name: "password-auth",
    description:
      "Traditional username/password authentication with PBKDF2-SHA256 hashing",
    dependencies: [],

    install(ctx: RelayContext): void {
      const manager = createPasswordAuthManager(options);
      ctx.setCapability(RELAY_CAPABILITIES.PASSWORD_AUTH, manager);
    },
  };
}
