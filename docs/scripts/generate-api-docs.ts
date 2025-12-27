/**
 * Generate API documentation from OpenAPI spec
 *
 * This script fetches the OpenAPI spec from a running Byonk server
 * and generates MDX documentation for the HTTP API.
 *
 * Usage:
 *   BYONK_URL=http://localhost:3000 npx tsx scripts/generate-api-docs.ts
 */

import * as fs from 'fs';
import * as path from 'path';

const BYONK_URL = process.env.BYONK_URL || 'http://localhost:3000';
const OUTPUT_FILE = path.join(__dirname, '../content/en/api/http-api.mdx');

interface OpenAPISpec {
  info: {
    title: string;
    description: string;
    version: string;
  };
  paths: Record<string, Record<string, PathOperation>>;
  components?: {
    schemas?: Record<string, Schema>;
  };
}

interface PathOperation {
  tags?: string[];
  summary?: string;
  description?: string;
  operationId?: string;
  parameters?: Parameter[];
  requestBody?: RequestBody;
  responses?: Record<string, Response>;
}

interface Parameter {
  name: string;
  in: string;
  description?: string;
  required?: boolean;
  schema?: Schema;
}

interface RequestBody {
  description?: string;
  required?: boolean;
  content?: Record<string, { schema?: Schema }>;
}

interface Response {
  description?: string;
  content?: Record<string, { schema?: Schema }>;
}

interface Schema {
  type?: string;
  properties?: Record<string, Schema>;
  items?: Schema;
  $ref?: string;
  description?: string;
  required?: string[];
}

function resolveRef(spec: OpenAPISpec, ref: string): Schema | undefined {
  if (!ref.startsWith('#/components/schemas/')) return undefined;
  const schemaName = ref.replace('#/components/schemas/', '');
  return spec.components?.schemas?.[schemaName];
}

function schemaToExample(spec: OpenAPISpec, schema: Schema, depth = 0): string {
  if (depth > 3) return '...';

  if (schema.$ref) {
    const resolved = resolveRef(spec, schema.$ref);
    if (resolved) return schemaToExample(spec, resolved, depth);
    return '{}';
  }

  if (schema.type === 'object' && schema.properties) {
    const props = Object.entries(schema.properties)
      .map(([key, val]) => `  "${key}": ${schemaToExample(spec, val, depth + 1)}`)
      .join(',\n');
    return `{\n${props}\n}`;
  }

  if (schema.type === 'array' && schema.items) {
    return `[${schemaToExample(spec, schema.items, depth + 1)}]`;
  }

  switch (schema.type) {
    case 'string':
      return '"string"';
    case 'integer':
    case 'number':
      return '0';
    case 'boolean':
      return 'true';
    default:
      return 'null';
  }
}

function generateEndpointDoc(
  spec: OpenAPISpec,
  path: string,
  method: string,
  op: PathOperation
): string {
  const lines: string[] = [];

  lines.push(`### ${method.toUpperCase()} ${path}`);
  lines.push('');

  if (op.summary) {
    lines.push(`**${op.summary}**`);
    lines.push('');
  }

  if (op.description) {
    lines.push(op.description);
    lines.push('');
  }

  // Parameters
  if (op.parameters && op.parameters.length > 0) {
    lines.push('#### Parameters');
    lines.push('');
    lines.push('| Name | In | Required | Description |');
    lines.push('|------|-----|----------|-------------|');
    for (const param of op.parameters) {
      const required = param.required ? 'Yes' : 'No';
      const desc = param.description || '-';
      lines.push(`| \`${param.name}\` | ${param.in} | ${required} | ${desc} |`);
    }
    lines.push('');
  }

  // Request Body
  if (op.requestBody?.content?.['application/json']?.schema) {
    lines.push('#### Request Body');
    lines.push('');
    lines.push('```json');
    lines.push(schemaToExample(spec, op.requestBody.content['application/json'].schema));
    lines.push('```');
    lines.push('');
  }

  // Responses
  if (op.responses) {
    lines.push('#### Responses');
    lines.push('');
    for (const [code, response] of Object.entries(op.responses)) {
      lines.push(`**${code}**: ${response.description || ''}`);
      if (response.content?.['application/json']?.schema) {
        lines.push('');
        lines.push('```json');
        lines.push(schemaToExample(spec, response.content['application/json'].schema));
        lines.push('```');
      }
      lines.push('');
    }
  }

  return lines.join('\n');
}

async function main() {
  console.log(`Fetching OpenAPI spec from ${BYONK_URL}/api-docs/openapi.json...`);

  let spec: OpenAPISpec;

  try {
    const response = await fetch(`${BYONK_URL}/api-docs/openapi.json`);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    spec = await response.json();
  } catch (error) {
    console.warn(`Could not fetch OpenAPI spec: ${error}`);
    console.log('Using fallback documentation...');

    // Generate fallback documentation
    const fallback = `---
title: HTTP API Reference
---

# HTTP API Reference

:::warning
This documentation was generated without access to the OpenAPI spec.
Start the Byonk server and run \`npm run generate-api\` to update.
:::

## Endpoints Overview

| Endpoint | Method | Description |
|----------|--------|-------------|
| \`/api/setup\` | GET | Device registration |
| \`/api/display\` | GET | Get display content (JSON with image URL) |
| \`/api/image/:device_id\` | GET | Get rendered PNG image |
| \`/api/log\` | POST | Device log submission |
| \`/health\` | GET | Health check |
| \`/swagger-ui\` | GET | Interactive API documentation |

For detailed API documentation, visit \`/swagger-ui\` on your running Byonk server.
`;

    fs.writeFileSync(OUTPUT_FILE, fallback);
    console.log(`Wrote fallback documentation to ${OUTPUT_FILE}`);
    return;
  }

  // Group endpoints by tag
  const byTag: Record<string, string[]> = {};

  for (const [path, methods] of Object.entries(spec.paths)) {
    for (const [method, op] of Object.entries(methods)) {
      if (typeof op !== 'object') continue;

      const tag = op.tags?.[0] || 'Other';
      if (!byTag[tag]) byTag[tag] = [];

      byTag[tag].push(generateEndpointDoc(spec, path, method, op));
    }
  }

  // Generate MDX
  const lines: string[] = [
    '---',
    'title: HTTP API Reference',
    '---',
    '',
    '# HTTP API Reference',
    '',
    `> ${spec.info.description}`,
    '',
    `**Version:** ${spec.info.version}`,
    '',
    '## Overview',
    '',
    'Byonk provides a REST API for TRMNL device communication. The API handles device registration, content delivery, and logging.',
    '',
    '| Endpoint | Description |',
    '|----------|-------------|',
    '| `GET /api/setup` | Device registration |',
    '| `GET /api/display` | Get display content URL |',
    '| `GET /api/image/:id` | Get rendered PNG |',
    '| `POST /api/log` | Submit device logs |',
    '| `GET /health` | Health check |',
    '',
  ];

  for (const [tag, docs] of Object.entries(byTag)) {
    lines.push(`## ${tag}`);
    lines.push('');
    for (const doc of docs) {
      lines.push(doc);
    }
  }

  // Ensure directory exists
  fs.mkdirSync(path.dirname(OUTPUT_FILE), { recursive: true });
  fs.writeFileSync(OUTPUT_FILE, lines.join('\n'));

  console.log(`Generated API documentation at ${OUTPUT_FILE}`);
}

main().catch(console.error);
