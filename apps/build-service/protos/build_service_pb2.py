# -*- coding: utf-8 -*-
# Generated by the protocol buffer compiler.  DO NOT EDIT!
# NO CHECKED-IN PROTOBUF GENCODE
# source: build_service.proto
# Protobuf Python Version: 5.29.0
"""Generated protocol buffer code."""
from google.protobuf import descriptor as _descriptor
from google.protobuf import descriptor_pool as _descriptor_pool
from google.protobuf import runtime_version as _runtime_version
from google.protobuf import symbol_database as _symbol_database
from google.protobuf.internal import builder as _builder
_runtime_version.ValidateProtobufRuntimeVersion(
    _runtime_version.Domain.PUBLIC,
    5,
    29,
    0,
    '',
    'build_service.proto'
)
# @@protoc_insertion_point(imports)

_sym_db = _symbol_database.Default()




DESCRIPTOR = _descriptor_pool.Default().AddSerializedFile(b'\n\x13\x62uild_service.proto\x12\x0c\x62uildservice\"a\n\x0c\x42uildRequest\x12\x0c\n\x04name\x18\x01 \x01(\t\x12\x10\n\x08git_repo\x18\x02 \x01(\t\x12\x17\n\x0f\x64ockerfile_path\x18\x03 \x01(\t\x12\x18\n\x10\x63ontext_sub_path\x18\x04 \x01(\t\"\x88\x01\n\rBuildResponse\x12\x32\n\x06status\x18\x01 \x01(\x0e\x32\".buildservice.BuildResponse.Status\x12\x0f\n\x07message\x18\x02 \x01(\t\x12\x10\n\x08\x62uild_id\x18\x03 \x01(\t\" \n\x06Status\x12\x0b\n\x07SUCCESS\x10\x00\x12\t\n\x05\x45RROR\x10\x01\"$\n\x10PollBuildRequest\x12\x10\n\x08\x62uild_id\x18\x01 \x01(\t\"\x85\x02\n\x11PollBuildResponse\x12\x36\n\x06status\x18\x01 \x01(\x0e\x32&.buildservice.PollBuildResponse.Status\x12\x0f\n\x07message\x18\x02 \x01(\t\x12\x41\n\x0c\x62uild_status\x18\x03 \x01(\x0e\x32+.buildservice.PollBuildResponse.BuildStatus\" \n\x06Status\x12\x0b\n\x07SUCCESS\x10\x00\x12\t\n\x05\x45RROR\x10\x01\"B\n\x0b\x42uildStatus\x12\x0b\n\x07UNKNOWN\x10\x00\x12\x0b\n\x07RUNNING\x10\x01\x12\r\n\tSUCCEEDED\x10\x02\x12\n\n\x06\x46\x41ILED\x10\x03\"\x1d\n\rDeployRequest\x12\x0c\n\x04name\x18\x01 \x01(\t\"x\n\x0e\x44\x65ployResponse\x12\x33\n\x06status\x18\x01 \x01(\x0e\x32#.buildservice.DeployResponse.Status\x12\x0f\n\x07message\x18\x02 \x01(\t\" \n\x06Status\x12\x0b\n\x07SUCCESS\x10\x00\x12\t\n\x05\x45RROR\x10\x01\x32\xe9\x01\n\x0c\x42uildService\x12\x42\n\x05\x42uild\x12\x1a.buildservice.BuildRequest\x1a\x1b.buildservice.BuildResponse\"\x00\x12N\n\tPollBuild\x12\x1e.buildservice.PollBuildRequest\x1a\x1f.buildservice.PollBuildResponse\"\x00\x12\x45\n\x06\x44\x65ploy\x12\x1b.buildservice.DeployRequest\x1a\x1c.buildservice.DeployResponse\"\x00\x62\x06proto3')

_globals = globals()
_builder.BuildMessageAndEnumDescriptors(DESCRIPTOR, _globals)
_builder.BuildTopDescriptorsAndMessages(DESCRIPTOR, 'build_service_pb2', _globals)
if not _descriptor._USE_C_DESCRIPTORS:
  DESCRIPTOR._loaded_options = None
  _globals['_BUILDREQUEST']._serialized_start=37
  _globals['_BUILDREQUEST']._serialized_end=134
  _globals['_BUILDRESPONSE']._serialized_start=137
  _globals['_BUILDRESPONSE']._serialized_end=273
  _globals['_BUILDRESPONSE_STATUS']._serialized_start=241
  _globals['_BUILDRESPONSE_STATUS']._serialized_end=273
  _globals['_POLLBUILDREQUEST']._serialized_start=275
  _globals['_POLLBUILDREQUEST']._serialized_end=311
  _globals['_POLLBUILDRESPONSE']._serialized_start=314
  _globals['_POLLBUILDRESPONSE']._serialized_end=575
  _globals['_POLLBUILDRESPONSE_STATUS']._serialized_start=241
  _globals['_POLLBUILDRESPONSE_STATUS']._serialized_end=273
  _globals['_POLLBUILDRESPONSE_BUILDSTATUS']._serialized_start=509
  _globals['_POLLBUILDRESPONSE_BUILDSTATUS']._serialized_end=575
  _globals['_DEPLOYREQUEST']._serialized_start=577
  _globals['_DEPLOYREQUEST']._serialized_end=606
  _globals['_DEPLOYRESPONSE']._serialized_start=608
  _globals['_DEPLOYRESPONSE']._serialized_end=728
  _globals['_DEPLOYRESPONSE_STATUS']._serialized_start=241
  _globals['_DEPLOYRESPONSE_STATUS']._serialized_end=273
  _globals['_BUILDSERVICE']._serialized_start=731
  _globals['_BUILDSERVICE']._serialized_end=964
# @@protoc_insertion_point(module_scope)
