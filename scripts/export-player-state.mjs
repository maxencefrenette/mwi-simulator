#!/usr/bin/env node

import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import process from "node:process";

const DEFAULT_CDP_URL = "http://127.0.0.1:9222";
const DEFAULT_OUTPUT = ".local/exports/player-state.json";

// Read-only CDP exporter. This script only evaluates JavaScript against the
// already-loaded MWI page and never clicks, types, navigates, or sends game API
// mutation requests.
const args = parseArgs(process.argv.slice(2));
const cdpUrl = args.cdpUrl ?? DEFAULT_CDP_URL;
const outputPath = resolve(args.output ?? DEFAULT_OUTPUT);

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--output") parsed.output = argv[++index];
    else if (arg === "--cdp-url") parsed.cdpUrl = argv[++index];
    else if (arg === "--help" || arg === "-h") {
      console.log(`Usage: node scripts/export-player-state.mjs [--cdp-url ${DEFAULT_CDP_URL}] [--output ${DEFAULT_OUTPUT}]`);
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  return parsed;
}

async function findMwiTarget(baseUrl) {
  const targets = await fetchJson(`${baseUrl.replace(/\/$/, "")}/json/list`);
  const target = targets.find(
    (candidate) => candidate.type === "page" && candidate.url.includes("milkywayidle.com"),
  );

  if (!target) {
    throw new Error(`No Milky Way Idle page found at ${baseUrl}. Run "mise run chrome-mwi" and log in first.`);
  }
  return target;
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`GET ${url} failed with ${response.status}`);
  return response.json();
}

class CdpClient {
  static async connect(webSocketDebuggerUrl) {
    const ws = new WebSocket(webSocketDebuggerUrl);
    const client = new CdpClient(ws);
    await client.open();
    return client;
  }

  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.ws.addEventListener("message", (event) => this.handleMessage(event));
  }

  open() {
    return new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
  }

  close() {
    this.ws.close();
  }

  handleMessage(event) {
    const message = JSON.parse(event.data);
    if (!message.id || !this.pending.has(message.id)) return;

    const { resolve, reject } = this.pending.get(message.id);
    this.pending.delete(message.id);

    if (message.error) reject(new Error(JSON.stringify(message.error)));
    else resolve(message.result);
  }

  send(method, params = {}) {
    const id = this.nextId;
    this.nextId += 1;

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      timeout: 10000,
    });

    if (result.exceptionDetails) {
      throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    }
    return result.result.value;
  }
}

async function main() {
  const target = await findMwiTarget(cdpUrl);
  const cdp = await CdpClient.connect(target.webSocketDebuggerUrl);

  try {
    const exportData = await cdp.evaluate(PLAYER_STATE_EXPORT_EXPRESSION);
    await mkdir(dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(exportData, null, 2)}\n`, "utf8");

    console.error(`Exported MWI player state to ${outputPath}`);
    console.error(`Character: ${exportData.character?.name ?? "unknown"} (${exportData.character?.id ?? "unknown"})`);
    console.error(`Inventory entries: ${Object.keys(exportData.characterItemMap ?? {}).length}`);
    console.error(`Open actions: ${(exportData.characterActions ?? []).length}`);
  } finally {
    cdp.close();
  }
}

const PLAYER_STATE_EXPORT_EXPRESSION = String.raw`
(() => {
  function findGameState() {
    const root = document.querySelector("#root")?._reactRootContainer?.current;
    const seenObjects = new WeakSet();
    const seenFibers = new WeakSet();

    function isGameState(value) {
      return Boolean(
        value &&
          typeof value === "object" &&
          value.character &&
          value.characterInfo &&
          value.characterActions &&
          value.characterSkillMap &&
          value.characterItemMap,
      );
    }

    function scanObject(value, depth = 0) {
      if (!value || typeof value !== "object" || seenObjects.has(value) || depth > 4) return null;
      seenObjects.add(value);
      if (isGameState(value)) return value;

      for (const key of Object.keys(value)) {
        if (!/props|state|value|data|game|character|player|current|memoized/i.test(key)) continue;
        const found = scanObject(value[key], depth + 1);
        if (found) return found;
      }
      return null;
    }

    function scanHooks(fiber) {
      let hook = fiber?.memoizedState;
      let index = 0;
      while (hook && index < 40) {
        const found = scanObject(hook.memoizedState);
        if (found) return found;
        hook = hook.next;
        index += 1;
      }
      return null;
    }

    function walkFiber(fiber) {
      if (!fiber || seenFibers.has(fiber)) return null;
      seenFibers.add(fiber);

      return (
        scanObject(fiber.memoizedState) ||
        scanObject(fiber.memoizedProps) ||
        scanHooks(fiber) ||
        walkFiber(fiber.child) ||
        walkFiber(fiber.sibling)
      );
    }

    return walkFiber(root);
  }

  function clone(value, seen = new WeakSet()) {
    if (value == null) return null;
    if (typeof value !== "object") return typeof value === "function" ? undefined : value;
    if (seen.has(value)) return "[Circular]";
    seen.add(value);

    let cloned;
    if (value instanceof Map) {
      cloned = Object.fromEntries(Array.from(value, ([key, mapValue]) => [String(key), clone(mapValue, seen)]));
    } else if (value instanceof Set) {
      cloned = Array.from(value, (setValue) => clone(setValue, seen));
    } else if (Array.isArray(value)) {
      cloned = value.map((item) => clone(item, seen)).filter((item) => item !== undefined);
    } else {
      cloned = Object.fromEntries(
        Object.entries(value)
          .map(([key, objectValue]) => [key, clone(objectValue, seen)])
          .filter(([, objectValue]) => objectValue !== undefined),
      );
    }

    seen.delete(value);
    return cloned;
  }

  function pick(value, keys) {
    if (!value) return null;
    return Object.fromEntries(keys.filter((key) => key in value).map((key) => [key, clone(value[key])]));
  }

  const state = findGameState();
  if (!state) throw new Error("Could not find MWI game state in the React tree");
  const inventoryItems = Array.from(state.characterItemMap?.values?.() ?? []).filter(
    (item) => item.itemLocationHrid === "/item_locations/inventory" && item.count > 0,
  );
  const marketListings = Array.from(state.myMarketListingMap?.values?.() ?? []);
  const activeMarketListings = marketListings.filter((listing) => listing.status === "/market_listing_status/active");

  function itemKey(itemHrid, enhancementLevel = 0) {
    const base = String(itemHrid ?? "").replace(/^\/items\//, "");
    return enhancementLevel > 0 ? base + ":" + enhancementLevel : base;
  }

  const coinItem = inventoryItems.find((item) => item.itemHrid === "/items/coin" && item.enhancementLevel === 0);

  return {
    schemaVersion: 1,
    exportedAt: new Date().toISOString(),
    source: {
      url: location.href,
      title: document.title,
      cdp: true,
      readOnly: true,
    },
    character: pick(state.character, [
      "id",
      "userID",
      "gameMode",
      "name",
      "isOnline",
      "lastOfflineTime",
      "inactiveTime",
      "createdAt",
      "updatedAt",
    ]),
    derived: {
      cash: coinItem?.count ?? 0,
      inventory: inventoryItems.map((item) => ({
        item: itemKey(item.itemHrid, item.enhancementLevel),
        itemHrid: item.itemHrid,
        enhancementLevel: item.enhancementLevel,
        quantity: item.count,
        location: item.itemLocationHrid,
        hash: item.hash,
      })),
      openOrders: activeMarketListings.map((listing) => {
        const remainingQuantity = Math.max(0, listing.orderQuantity - listing.filledQuantity);
        return {
          id: listing.id,
          side: listing.isSell ? "sell" : "buy",
          item: itemKey(listing.itemHrid, listing.enhancementLevel),
          itemHrid: listing.itemHrid,
          enhancementLevel: listing.enhancementLevel,
          quantity: remainingQuantity,
          orderQuantity: listing.orderQuantity,
          filledQuantity: listing.filledQuantity,
          limitPrice: listing.price,
          lockedCash: listing.isSell ? 0 : listing.coinsAvailable,
          status: listing.status,
          createdAt: listing.createdTimestamp,
          expiresAt: listing.expirationTimestamp,
        };
      }),
    },
    characterInfo: clone(state.characterInfo),
    characterActions: clone(state.characterActions),
    characterSkillMap: clone(state.characterSkillMap),
    characterItemMap: clone(state.characterItemMap),
    characterItemByLocationMap: clone(state.characterItemByLocationMap),
    characterLoadoutDict: clone(state.characterLoadoutDict),
    actionTypeFoodSlotsDict: clone(state.actionTypeFoodSlotsDict),
    actionTypeDrinkSlotsDict: clone(state.actionTypeDrinkSlotsDict),
    skillingActionTypeBuffsDict: clone(state.skillingActionTypeBuffsDict),
    skillingActionHridBuffsDict: clone(state.skillingActionHridBuffsDict),
    mooPassActionTypeBuffsDict: clone(state.mooPassActionTypeBuffsDict),
    communityActionTypeBuffsDict: clone(state.communityActionTypeBuffsDict),
    consumableActionTypeBuffsDict: clone(state.consumableActionTypeBuffsDict),
    equipmentActionTypeBuffsDict: clone(state.equipmentActionTypeBuffsDict),
    houseActionTypeBuffsDict: clone(state.houseActionTypeBuffsDict),
    activeCharacterQuest: clone(state.activeCharacterQuest),
    characterQuestMap: clone(state.characterQuestMap),
    characterHouseRoomDict: clone(state.characterHouseRoomDict),
    myMarketListingMap: clone(state.myMarketListingMap),
    marketItemOrderBooks: clone(state.marketItemOrderBooks),
    actionDetailMaps: clone(state.actionDetailMaps),
    skillDetailDict: clone(state.skillDetailDict),
    itemDetailDict: clone(state.itemDetailDict),
    itemCategoryDetailDict: clone(state.itemCategoryDetailDict),
    itemLocationDetailDict: clone(state.itemLocationDetailDict),
  };
})()
`;

await main();
