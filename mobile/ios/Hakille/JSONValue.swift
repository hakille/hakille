import Foundation

/// Protocol IDs stay UInt64 all the way through decoding and encoding.
indirect enum JSONValue: Codable, Equatable, Sendable {
    case object([String: JSONValue]), array([JSONValue]), string(String), integer(Int64), unsigned(UInt64), decimal(Double), bool(Bool), null
    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if value.decodeNil() { self = .null }
        else if let item = try? value.decode(Bool.self) { self = .bool(item) }
        else if let item = try? value.decode(Int64.self) { self = .integer(item) }
        else if let item = try? value.decode(UInt64.self) { self = .unsigned(item) }
        else if let item = try? value.decode(Double.self) { self = .decimal(item) }
        else if let item = try? value.decode(String.self) { self = .string(item) }
        else if let item = try? value.decode([String: JSONValue].self) { self = .object(item) }
        else { self = .array(try value.decode([JSONValue].self)) }
    }
    func encode(to encoder: Encoder) throws {
        var value = encoder.singleValueContainer()
        switch self {
        case .object(let v): try value.encode(v)
        case .array(let v): try value.encode(v)
        case .string(let v): try value.encode(v)
        case .integer(let v): try value.encode(v)
        case .unsigned(let v): try value.encode(v)
        case .decimal(let v): try value.encode(v)
        case .bool(let v): try value.encode(v)
        case .null: try value.encodeNil()
        }
    }
    subscript(_ key: String) -> JSONValue { if case .object(let v) = self { return v[key] ?? .null }; return .null }
    var text: String {
        switch self {
        case .string(let v): return v
        case .integer(let v): return String(v)
        case .unsigned(let v): return String(v)
        case .bool(let v): return String(v)
        case .null: return ""
        default: return (try? encoded()) ?? ""
        }
    }
    var array: [JSONValue] { if case .array(let values) = self { return values }; return [] }
    var object: [String: JSONValue] { if case .object(let values) = self { return values }; return [:] }
    var id: UInt64? { UInt64(text) }
    var isNull: Bool { self == .null }
    static func parse(_ json: String) throws -> JSONValue { try JSONDecoder().decode(Self.self, from: Data(json.utf8)) }
    func encoded() throws -> String { String(decoding: try JSONEncoder().encode(self), as: UTF8.self) }
}
