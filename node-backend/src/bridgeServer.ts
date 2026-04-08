import { createServer, IncomingMessage, ServerResponse } from 'node:http';
import { createBridgeHandler } from './openai-bridge/index.js';

export const DEFAULT_BRIDGE_PORT = Number(process.env.CLAUDE_DESK_BRIDGE_PORT || 53686);
const SUPPORTED_UPSTREAM_FORMATS = ['chat_completions', 'responses'] as const;
const DEFAULT_UPSTREAM_FORMAT = 'chat_completions';

interface EncodedBridgeConfig {
  baseUrl: string;
  upstreamFormat?: 'chat_completions' | 'responses';
  model?: string;
}

function decodeConfig(segment: string): EncodedBridgeConfig {
  const normalized = segment.replace(/-/g, '+').replace(/_/g, '/');
  const padded = normalized + '='.repeat((4 - (normalized.length % 4)) % 4);
  const json = Buffer.from(padded, 'base64').toString('utf8');
  return JSON.parse(json) as EncodedBridgeConfig;
}

async function readBody(req: IncomingMessage): Promise<Buffer> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks);
}

export async function startBridgeServer(port: number = DEFAULT_BRIDGE_PORT): Promise<void> {
  const bridgeHandler = createBridgeHandler({
    getUpstreamConfig: async (request) => {
      const url = new URL(request.url);
      const parts = url.pathname.split('/').filter(Boolean);
      const encodedConfig = parts[1];
      const config = decodeConfig(encodedConfig);
      return {
        baseUrl: config.baseUrl,
        model: config.model,
        upstreamFormat: config.upstreamFormat || DEFAULT_UPSTREAM_FORMAT,
      };
    },
    logger: (msg) => console.error(`[OpenAIBridge] ${msg}`),
  });

  const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
    try {
      const host = req.headers.host || `127.0.0.1:${port}`;
      const url = `http://${host}${req.url || '/'}`;

      if (req.method === 'GET' && req.url === '/health') {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ ok: true }));
        return;
      }

      if (req.method !== 'POST' || !req.url?.includes('/v1/messages')) {
        res.writeHead(404, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ error: 'Not found' }));
        return;
      }

      const body = await readBody(req);
      const request = new Request(url, {
        method: req.method,
        headers: new Headers(req.headers as Record<string, string>),
        body: body.length ? new Uint8Array(body) : undefined,
      });

      const response = await bridgeHandler(request);
      const headers = Object.fromEntries(response.headers.entries());
      res.writeHead(response.status, headers);

      if (!response.body) {
        res.end();
        return;
      }

      const reader = response.body.getReader();
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        res.write(Buffer.from(value));
      }
      res.end();
    } catch (error) {
      console.error('[OpenAIBridge] request error:', error);
      res.writeHead(500, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ error: error instanceof Error ? error.message : 'bridge error' }));
    }
  });

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve());
  });

  console.error(
    `[OpenAIBridge] listening on http://127.0.0.1:${port} upstreamFormat.default=${DEFAULT_UPSTREAM_FORMAT} upstreamFormat.supported=${SUPPORTED_UPSTREAM_FORMATS.join(',')}`,
  );
}
