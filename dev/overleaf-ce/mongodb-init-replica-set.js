/* eslint-disable no-undef */
// runs once when the mongo volume is first created
rs.initiate({ _id: 'overleaf', members: [{ _id: 0, host: 'mongo:27017' }] })
